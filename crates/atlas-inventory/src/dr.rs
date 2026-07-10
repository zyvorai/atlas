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
        "SELECT id, tenant_id, volume_id, pool, image, peer_id, mode, role, state, rpo_seconds, updated_at
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
                "updated_at": r.get::<String, _>("updated_at"),
            })
        })
        .collect())
}
