// Copyright (c) 2026 ZyvorAI Labs Private Limited. All rights reserved.
//! Cross-cluster DR state: mirroring peers + per-image mirror records (day-2 scaffolding). The real
//! `rbd mirror` operations run as jobs; this is the control-plane catalog they update.

use anyhow::Result;
use sqlx::{Row, SqlitePool};

// ---- peers ----

/// Register (or update) a mirroring peer cluster. `secret_ref` names a k8s Secret with its bootstrap
/// token — never the token itself.
pub async fn register_peer(
    pool: &SqlitePool,
    id: &str,
    name: &str,
    cluster_fsid: Option<&str>,
    direction: &str,
    secret_ref: Option<&str>,
) -> Result<()> {
    sqlx::query(
        "INSERT INTO dr_peers (id, name, cluster_fsid, direction, bootstrap_secret_ref, state)
         VALUES (?, ?, ?, ?, ?, 'registered')
         ON CONFLICT(id) DO UPDATE SET name=excluded.name, cluster_fsid=excluded.cluster_fsid,
            direction=excluded.direction, bootstrap_secret_ref=excluded.bootstrap_secret_ref",
    )
    .bind(id)
    .bind(name)
    .bind(cluster_fsid)
    .bind(direction)
    .bind(secret_ref)
    .execute(pool)
    .await?;
    Ok(())
}

/// Remove a mirroring peer and any mirrors that referenced it (stale/decommissioned peer).
pub async fn delete_peer(pool: &SqlitePool, id: &str) -> Result<()> {
    sqlx::query("DELETE FROM dr_mirrors WHERE peer_id=?").bind(id).execute(pool).await?;
    sqlx::query("DELETE FROM dr_peers WHERE id=?").bind(id).execute(pool).await?;
    Ok(())
}

pub async fn list_peers(pool: &SqlitePool) -> Result<Vec<serde_json::Value>> {
    let rows = sqlx::query(
        "SELECT id, name, cluster_fsid, direction, bootstrap_secret_ref, state, created_at
         FROM dr_peers ORDER BY created_at DESC",
    )
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| {
            serde_json::json!({
                "id": r.get::<String, _>("id"),
                "name": r.get::<String, _>("name"),
                "cluster_fsid": r.get::<Option<String>, _>("cluster_fsid"),
                "direction": r.get::<String, _>("direction"),
                "bootstrap_secret_ref": r.get::<Option<String>, _>("bootstrap_secret_ref"),
                "state": r.get::<String, _>("state"),
            })
        })
        .collect())
}

// ---- mirrors ----

/// Record (or update) a mirrored image. Keyed uniquely by `(pool, image)`.
#[allow(clippy::too_many_arguments)]
pub async fn upsert_mirror(
    pool: &SqlitePool,
    id: &str,
    tenant_id: &str,
    volume_id: Option<&str>,
    rbd_pool: &str,
    image: &str,
    peer_id: Option<&str>,
    mode: &str,
    role: &str,
    state: &str,
) -> Result<()> {
    sqlx::query(
        "INSERT INTO dr_mirrors (id, tenant_id, volume_id, pool, image, peer_id, mode, role, state)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
         ON CONFLICT(pool, image) DO UPDATE SET peer_id=excluded.peer_id, mode=excluded.mode,
            role=excluded.role, state=excluded.state,
            updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now')",
    )
    .bind(id)
    .bind(tenant_id)
    .bind(volume_id)
    .bind(rbd_pool)
    .bind(image)
    .bind(peer_id)
    .bind(mode)
    .bind(role)
    .bind(state)
    .execute(pool)
    .await?;
    Ok(())
}

/// Update a mirror's role and/or state. Returns whether a row changed.
pub async fn set_mirror(pool: &SqlitePool, id: &str, role: &str, state: &str) -> Result<bool> {
    let res = sqlx::query(
        "UPDATE dr_mirrors SET role=?, state=?, updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE id=?",
    )
    .bind(role)
    .bind(state)
    .bind(id)
    .execute(pool)
    .await?;
    Ok(res.rows_affected() > 0)
}

/// Update only `state`, leaving `role` untouched. Used by the enable job so it cannot race a
/// concurrent demote/promote that already moved the role.
pub async fn set_mirror_state(pool: &SqlitePool, id: &str, state: &str) -> Result<bool> {
    let res = sqlx::query(
        "UPDATE dr_mirrors SET state=?, updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE id=?",
    )
    .bind(state)
    .bind(id)
    .execute(pool)
    .await?;
    Ok(res.rows_affected() > 0)
}

/// A mirror's `(pool, image, role)` — used by the promote/demote/disable jobs.
pub async fn mirror_target(pool: &SqlitePool, id: &str) -> Result<Option<(String, String, String)>> {
    let row = sqlx::query("SELECT pool, image, role FROM dr_mirrors WHERE id=?")
        .bind(id)
        .fetch_optional(pool)
        .await?;
    Ok(row.map(|r| (r.get("pool"), r.get("image"), r.get("role"))))
}

pub async fn list_mirrors(pool: &SqlitePool) -> Result<Vec<serde_json::Value>> {
    let rows = sqlx::query(
        "SELECT id, tenant_id, volume_id, pool, image, peer_id, mode, role, state, rpo_seconds,
                last_failover_at, last_error, force_promoted, updated_at
         FROM dr_mirrors ORDER BY created_at DESC",
    )
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| {
            serde_json::json!({
                "id": r.get::<String, _>("id"),
                "tenant_id": r.get::<String, _>("tenant_id"),
                "volume_id": r.get::<Option<String>, _>("volume_id"),
                "pool": r.get::<String, _>("pool"),
                "image": r.get::<String, _>("image"),
                "peer_id": r.get::<Option<String>, _>("peer_id"),
                "mode": r.get::<String, _>("mode"),
                "role": r.get::<String, _>("role"),
                "state": r.get::<String, _>("state"),
                "rpo_seconds": r.get::<Option<i64>, _>("rpo_seconds"),
                "last_failover_at": r.get::<Option<String>, _>("last_failover_at"),
                "last_error": r.get::<Option<String>, _>("last_error"),
                "force_promoted": r.get::<i64, _>("force_promoted") != 0,
                "updated_at": r.get::<String, _>("updated_at"),
            })
        })
        .collect())
}

/// Whether a peer id exists in the catalog.
pub async fn peer_exists(pool: &SqlitePool, id: &str) -> Result<bool> {
    let n: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM dr_peers WHERE id=?")
        .bind(id)
        .fetch_one(pool)
        .await?;
    Ok(n > 0)
}

/// Full mirror row for transition guards: `(pool, image, role, state)`.
pub async fn mirror_detail(
    pool: &SqlitePool,
    id: &str,
) -> Result<Option<(String, String, String, String)>> {
    let row = sqlx::query("SELECT pool, image, role, state FROM dr_mirrors WHERE id=?")
        .bind(id)
        .fetch_optional(pool)
        .await?;
    Ok(row.map(|r| {
        (
            r.get("pool"),
            r.get("image"),
            r.get("role"),
            r.get("state"),
        )
    }))
}

/// Stamp an error onto a mirror (real `rbd mirror` failure) without changing role.
pub async fn set_mirror_error(pool: &SqlitePool, id: &str, error: &str) -> Result<()> {
    sqlx::query(
        "UPDATE dr_mirrors SET state='error', last_error=?,
         updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE id=?",
    )
    .bind(error)
    .bind(id)
    .execute(pool)
    .await?;
    Ok(())
}

/// Record a successful promote/failover drill.
pub async fn record_failover(pool: &SqlitePool, id: &str, force: bool) -> Result<()> {
    sqlx::query(
        "UPDATE dr_mirrors SET last_failover_at=strftime('%Y-%m-%dT%H:%M:%fZ','now'),
         last_error=NULL, force_promoted=?,
         updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE id=?",
    )
    .bind(if force { 1 } else { 0 })
    .bind(id)
    .execute(pool)
    .await?;
    Ok(())
}

/// Set observed RPO seconds for a mirrored image (operator/refresh path).
pub async fn set_rpo(pool: &SqlitePool, id: &str, rpo_seconds: Option<i64>) -> Result<bool> {
    let res = sqlx::query(
        "UPDATE dr_mirrors SET rpo_seconds=?, updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE id=?",
    )
    .bind(rpo_seconds)
    .bind(id)
    .execute(pool)
    .await?;
    Ok(res.rows_affected() > 0)
}

/// Preflight checklist for a DR failover drill (control-plane view; does not talk to Ceph).
///
/// `ready` means the **catalog** is coherent enough to run a failover *job*. It does **not** mean
/// live `rbd mirror` dataplane has been verified — that stays `dataplane_verified: false` until a
/// two-site drill lands (see `docs/DR.md`).
pub async fn preflight(pool: &SqlitePool) -> Result<serde_json::Value> {
    let peers = list_peers(pool).await?;
    let mirrors = list_mirrors(pool).await?;
    let mut checks = Vec::new();
    let mut blockers = Vec::new();
    let mut warnings = Vec::new();

    let peer_ok = !peers.is_empty();
    checks.push(serde_json::json!({
        "id": "peer_registered", "ok": peer_ok,
        "detail": if peer_ok { format!("{} peer(s)", peers.len()) } else { "no DR peers registered".into() }
    }));
    if !peer_ok {
        blockers.push("register at least one peer via POST /dr/peers");
    }

    let with_secret = peers
        .iter()
        .filter(|p| {
            p["bootstrap_secret_ref"]
                .as_str()
                .map(|s| !s.is_empty())
                .unwrap_or(false)
        })
        .count();
    let secret_ok = !peer_ok || with_secret == peers.len();
    checks.push(serde_json::json!({
        "id": "peer_secret_refs", "ok": secret_ok,
        "detail": format!("{with_secret}/{} peers have bootstrap_secret_ref", peers.len())
    }));
    if !secret_ok {
        blockers.push("every peer should reference a k8s Secret holding the bootstrap token");
    }

    let errored: Vec<_> = mirrors
        .iter()
        .filter(|m| m["state"] == "error")
        .map(|m| m["id"].as_str().unwrap_or("").to_string())
        .collect();
    let err_ok = errored.is_empty();
    checks.push(serde_json::json!({
        "id": "no_mirror_errors", "ok": err_ok,
        "detail": if err_ok { "no mirrors in error state".into() } else { format!("error: {}", errored.join(",")) }
    }));
    if !err_ok {
        blockers.push("resolve mirrors in state=error before failing over");
    }

    let enabled = mirrors.iter().filter(|m| m["state"] == "enabled").count();
    let enabled_ok = mirrors.is_empty() || enabled > 0;
    checks.push(serde_json::json!({
        "id": "enabled_mirrors", "ok": enabled_ok,
        "detail": format!("{enabled}/{} mirror(s) in state=enabled", mirrors.len())
    }));
    if !enabled_ok {
        blockers.push("no enabled mirrors — enable mirroring on a volume before failover");
    }

    let secondaries = mirrors.iter().filter(|m| m["role"] == "secondary").count();
    let failover_ready = secondaries > 0;
    checks.push(serde_json::json!({
        "id": "secondary_available", "ok": failover_ready || mirrors.is_empty(),
        "detail": format!("{secondaries} secondary mirror(s) can be promoted")
    }));
    if !failover_ready && !mirrors.is_empty() {
        warnings.push("no secondary role yet — demote a primary (or wait for peer sync) before promote");
    }

    let with_rpo = mirrors.iter().filter(|m| m["rpo_seconds"].as_i64().is_some()).count();
    let rpo_ok = mirrors.is_empty() || with_rpo > 0;
    checks.push(serde_json::json!({
        "id": "rpo_observed", "ok": rpo_ok,
        "detail": if mirrors.is_empty() {
            "no mirrors yet".into()
        } else {
            format!("{with_rpo}/{} mirror(s) have an observed RPO", mirrors.len())
        }
    }));
    if !rpo_ok {
        warnings.push("record observed RPO via POST /dr/mirrors/{id}/rpo after a live sync sample");
    }

    // Honest: control-plane catalog only. Live rbd mirror needs a second Ceph cluster.
    checks.push(serde_json::json!({
        "id": "dataplane_verified", "ok": false,
        "detail": "live two-site rbd mirror verification pending — see docs/DR.md"
    }));
    warnings.push("dataplane unverified: treat failover as a control-plane drill until a peer cluster exists");

    Ok(serde_json::json!({
        "ready": blockers.is_empty(),
        "control_plane_ready": blockers.is_empty(),
        "dataplane_verified": false,
        "checks": checks,
        "blockers": blockers,
        "warnings": warnings,
        "peers": peers.len(),
        "mirrors": mirrors.len(),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn mem_pool() -> SqlitePool {
        let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
        sqlx::query(
            "CREATE TABLE dr_peers (
                id TEXT PRIMARY KEY, name TEXT NOT NULL, cluster_fsid TEXT,
                direction TEXT NOT NULL, bootstrap_secret_ref TEXT, state TEXT NOT NULL,
                created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now'))
             )",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "CREATE TABLE dr_mirrors (
                id TEXT PRIMARY KEY, tenant_id TEXT NOT NULL, volume_id TEXT,
                pool TEXT NOT NULL, image TEXT NOT NULL, peer_id TEXT,
                mode TEXT NOT NULL, role TEXT NOT NULL, state TEXT NOT NULL,
                rpo_seconds INTEGER, last_failover_at TEXT, last_error TEXT,
                force_promoted INTEGER NOT NULL DEFAULT 0,
                created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
                updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
                UNIQUE(pool, image)
             )",
        )
        .execute(&pool)
        .await
        .unwrap();
        pool
    }

    #[tokio::test]
    async fn preflight_blocks_without_peers_and_stays_dataplane_unverified() {
        let pool = mem_pool().await;
        let pre = preflight(&pool).await.unwrap();
        assert_eq!(pre["ready"], false);
        assert_eq!(pre["dataplane_verified"], false);
        assert!(pre["blockers"].as_array().unwrap().iter().any(|b| b.as_str().unwrap().contains("peer")));

        register_peer(&pool, "p1", "dc2", Some("fsid"), "bidirectional", Some("sec")).await.unwrap();
        let pre2 = preflight(&pool).await.unwrap();
        assert_eq!(pre2["ready"], true);
        assert_eq!(pre2["control_plane_ready"], true);
        assert_eq!(pre2["dataplane_verified"], false);
        assert!(pre2["warnings"].as_array().unwrap().iter().any(|w| {
            w.as_str().unwrap().contains("dataplane unverified")
        }));
    }
}

