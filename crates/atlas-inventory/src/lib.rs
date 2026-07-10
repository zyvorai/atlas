// Copyright (c) 2026 ZyvorAI Labs Private Limited. All rights reserved.
//! SQLite-backed normalized inventory: connect/migrate, upsert-from-discovery, and read queries
//! that power the gateway's read-only endpoints.
//!
//! Connect/migrate mirrors `machina/controller/src/db/mod.rs` (WAL, `create_if_missing`,
//! `sqlx::migrate!`). Read queries return `atlas-api-types` DTOs.

use std::str::FromStr;
use std::time::Duration;

use anyhow::Result;
use atlas_api_types::{
    BackendMode, BackendType, Capabilities, DiscoveryResult, Health, Osd, StorageBackend,
    StorageCluster, StorageHealth, StoragePool, StorageVolume, VolumeKind,
};
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions};
use sqlx::{Row, SqlitePool};

pub mod alerts;
pub mod audit;
pub mod backups;
pub mod dr;
pub mod buckets;
pub mod databridge;
pub mod events;
pub mod jobs;
pub mod leader;
pub mod metrics;
pub mod schedules;
pub mod snapshots;
pub mod tokens;
pub mod tenants;

/// Open the SQLite pool with WAL + foreign keys, creating the file if missing.
pub async fn connect(database_url: &str) -> Result<SqlitePool> {
    let options = SqliteConnectOptions::from_str(database_url)?
        .create_if_missing(true)
        .journal_mode(SqliteJournalMode::Wal)
        .pragma("foreign_keys", "ON")
        .busy_timeout(Duration::from_secs(5));
    let pool = SqlitePoolOptions::new()
        .max_connections(4)
        .connect_with(options)
        .await?;
    Ok(pool)
}

/// Run the embedded migrations (from the workspace-root `migrations/` dir).
pub async fn migrate(pool: &SqlitePool) -> Result<()> {
    sqlx::migrate!("../../migrations").run(pool).await?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Enum <-> TEXT helpers
// ---------------------------------------------------------------------------

fn health_str(h: Health) -> &'static str {
    match h {
        Health::Ok => "ok",
        Health::Warn => "warn",
        Health::Critical => "critical",
        Health::Unknown => "unknown",
    }
}
fn health_from(s: &str) -> Health {
    match s {
        "ok" => Health::Ok,
        "warn" => Health::Warn,
        "critical" => Health::Critical,
        _ => Health::Unknown,
    }
}
fn backend_type_str(t: BackendType) -> &'static str {
    match t {
        BackendType::Ceph => "ceph",
        BackendType::Nfs => "nfs",
        BackendType::Zfs => "zfs",
        BackendType::San => "san",
        BackendType::CloudBlock => "cloud_block",
        BackendType::Kubernetes => "kubernetes",
    }
}
fn backend_type_from(s: &str) -> BackendType {
    match s {
        "nfs" => BackendType::Nfs,
        "zfs" => BackendType::Zfs,
        "san" => BackendType::San,
        "cloud_block" => BackendType::CloudBlock,
        "kubernetes" => BackendType::Kubernetes,
        _ => BackendType::Ceph,
    }
}
fn mode_str(m: BackendMode) -> &'static str {
    match m {
        BackendMode::ManagedRook => "managed_rook",
        BackendMode::External => "external",
        BackendMode::ReadOnly => "read_only",
    }
}
fn mode_from(s: &str) -> BackendMode {
    match s {
        "managed_rook" => BackendMode::ManagedRook,
        "read_only" => BackendMode::ReadOnly,
        _ => BackendMode::External,
    }
}
fn volume_kind_str(k: VolumeKind) -> &'static str {
    match k {
        VolumeKind::Block => "block",
        VolumeKind::Filesystem => "filesystem",
        VolumeKind::Object => "object",
    }
}
fn volume_kind_from(s: &str) -> VolumeKind {
    match s {
        "filesystem" => VolumeKind::Filesystem,
        "object" => VolumeKind::Object,
        _ => VolumeKind::Block,
    }
}

// ---------------------------------------------------------------------------
// Writes
// ---------------------------------------------------------------------------

/// Insert or update a backend registration row.
pub async fn upsert_backend(pool: &SqlitePool, b: &StorageBackend) -> Result<()> {
    let caps = serde_json::to_string(&b.capabilities)?;
    sqlx::query(
        "INSERT INTO storage_backends (id, name, backend_type, mode, status, capabilities, connection_ref, updated_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, strftime('%Y-%m-%dT%H:%M:%fZ','now'))
         ON CONFLICT(id) DO UPDATE SET
            name=excluded.name, backend_type=excluded.backend_type, mode=excluded.mode,
            status=excluded.status, capabilities=excluded.capabilities,
            connection_ref=excluded.connection_ref, updated_at=excluded.updated_at",
    )
    .bind(&b.id)
    .bind(&b.name)
    .bind(backend_type_str(b.backend_type))
    .bind(mode_str(b.mode))
    .bind(&b.status)
    .bind(caps)
    .bind(&b.connection_ref)
    .execute(pool)
    .await?;
    Ok(())
}

/// Insert or update a single volume (write path). Sets tenant/policy which discovery leaves default.
pub async fn upsert_volume(
    pool: &SqlitePool,
    backend_id: &str,
    tenant_id: &str,
    v: &StorageVolume,
    policy_id: Option<&str>,
) -> Result<()> {
    sqlx::query(
        "INSERT INTO storage_volumes
            (id, tenant_id, backend_id, cluster_id, pool_id, name, kind, backend_native_id, size_bytes, used_bytes,
             state, health, policy_id, kubernetes_namespace, pvc_name, storage_class_name, updated_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, strftime('%Y-%m-%dT%H:%M:%fZ','now'))
         ON CONFLICT(id) DO UPDATE SET
            tenant_id=excluded.tenant_id, cluster_id=excluded.cluster_id, pool_id=excluded.pool_id,
            name=excluded.name, kind=excluded.kind, backend_native_id=excluded.backend_native_id,
            size_bytes=excluded.size_bytes, used_bytes=excluded.used_bytes, state=excluded.state,
            health=excluded.health, policy_id=excluded.policy_id, kubernetes_namespace=excluded.kubernetes_namespace,
            pvc_name=excluded.pvc_name, storage_class_name=excluded.storage_class_name, updated_at=excluded.updated_at",
    )
    .bind(&v.id)
    .bind(tenant_id)
    .bind(backend_id)
    .bind(&v.cluster_id)
    .bind(&v.pool_id)
    .bind(&v.name)
    .bind(volume_kind_str(v.kind))
    .bind(&v.backend_native_id)
    .bind(v.size_bytes)
    .bind(v.used_bytes)
    .bind(&v.state)
    .bind(health_str(v.health))
    .bind(policy_id)
    .bind(&v.kubernetes_namespace)
    .bind(&v.pvc_name)
    .bind(&v.storage_class_name)
    .execute(pool)
    .await?;
    Ok(())
}

/// Link a volume to the snapshot it was cloned/restored from (dependency tracking).
pub async fn set_volume_source_snapshot(
    pool: &SqlitePool,
    volume_id: &str,
    snapshot_id: &str,
) -> Result<()> {
    sqlx::query("UPDATE storage_volumes SET source_snapshot_id=? WHERE id=?")
        .bind(snapshot_id)
        .bind(volume_id)
        .execute(pool)
        .await?;
    Ok(())
}

/// How many volumes were cloned/restored from a snapshot (blocks unsafe snapshot deletion).
pub async fn count_snapshot_dependents(pool: &SqlitePool, snapshot_id: &str) -> Result<i64> {
    let n: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM storage_volumes WHERE source_snapshot_id=?")
            .bind(snapshot_id)
            .fetch_one(pool)
            .await?;
    Ok(n)
}

/// Update a volume's provisioned size (after an expand/resize).
pub async fn set_volume_size(pool: &SqlitePool, id: &str, size_bytes: i64) -> Result<()> {
    sqlx::query(
        "UPDATE storage_volumes SET size_bytes=?, updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE id=?",
    )
    .bind(size_bytes)
    .bind(id)
    .execute(pool)
    .await?;
    Ok(())
}

/// Record a volume's applied QoS limits under `metadata.qos` (day-2 throttling).
pub async fn set_volume_qos(
    pool: &SqlitePool,
    id: &str,
    iops_limit: Option<i64>,
    bps_limit: Option<i64>,
) -> Result<()> {
    let qos = serde_json::json!({ "iops_limit": iops_limit, "bps_limit": bps_limit });
    sqlx::query(
        "UPDATE storage_volumes SET metadata = json_set(metadata, '$.qos', json(?)),
         updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE id=?",
    )
    .bind(qos.to_string())
    .bind(id)
    .execute(pool)
    .await?;
    Ok(())
}

/// Update a volume's actual used (allocated) bytes.
pub async fn set_volume_used(pool: &SqlitePool, id: &str, used_bytes: i64) -> Result<()> {
    sqlx::query(
        "UPDATE storage_volumes SET used_bytes=?, updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE id=?",
    )
    .bind(used_bytes)
    .bind(id)
    .execute(pool)
    .await?;
    Ok(())
}

/// Update just the state of a volume (e.g. to `deleting`).
pub async fn set_volume_state(pool: &SqlitePool, id: &str, state: &str) -> Result<()> {
    sqlx::query(
        "UPDATE storage_volumes SET state=?, updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE id=?",
    )
    .bind(state)
    .bind(id)
    .execute(pool)
    .await?;
    Ok(())
}

/// Delete a volume row (after the backend resource is gone).
pub async fn delete_volume_row(pool: &SqlitePool, id: &str) -> Result<()> {
    sqlx::query("DELETE FROM storage_volumes WHERE id=?")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

/// Record product ownership of a storage resource (PDF §5.5 ownership mapping).
#[allow(clippy::too_many_arguments)]
pub async fn insert_binding(
    pool: &SqlitePool,
    id: &str,
    tenant_id: &str,
    product: &str,
    resource_type: &str,
    resource_id: &str,
    storage_resource_type: &str,
    storage_resource_id: &str,
    role: &str,
) -> Result<()> {
    sqlx::query(
        "INSERT INTO product_bindings
            (id, tenant_id, product, resource_type, resource_id, storage_resource_type, storage_resource_id, role)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?)
         ON CONFLICT(product, resource_type, resource_id, storage_resource_type, storage_resource_id, role) DO NOTHING",
    )
    .bind(id)
    .bind(tenant_id)
    .bind(product)
    .bind(resource_type)
    .bind(resource_id)
    .bind(storage_resource_type)
    .bind(storage_resource_id)
    .bind(role)
    .execute(pool)
    .await?;
    Ok(())
}

/// List the product ownership bindings for a storage resource (e.g. a volume), newest first.
pub async fn list_bindings_for(
    pool: &SqlitePool,
    storage_resource_type: &str,
    storage_resource_id: &str,
) -> Result<Vec<serde_json::Value>> {
    let rows = sqlx::query(
        "SELECT id, tenant_id, product, resource_type, resource_id, role, created_at
         FROM product_bindings
         WHERE storage_resource_type = ? AND storage_resource_id = ?
         ORDER BY created_at DESC",
    )
    .bind(storage_resource_type)
    .bind(storage_resource_id)
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| {
            serde_json::json!({
                "id": r.get::<String, _>("id"),
                "tenant_id": r.get::<String, _>("tenant_id"),
                "product": r.get::<String, _>("product"),
                "resource_type": r.get::<String, _>("resource_type"),
                "resource_id": r.get::<String, _>("resource_id"),
                "role": r.get::<String, _>("role"),
                "created_at": r.get::<String, _>("created_at"),
            })
        })
        .collect())
}

/// Persist a full discovery pass for `backend_id`, upserting cluster, pools, osds and volumes.
pub async fn upsert_discovery(
    pool: &SqlitePool,
    backend_id: &str,
    d: &DiscoveryResult,
) -> Result<()> {
    let mut tx = pool.begin().await?;

    // cluster
    sqlx::query(
        "INSERT INTO storage_clusters
            (id, backend_id, native_fsid, name, health, raw_capacity_bytes, used_capacity_bytes, available_capacity_bytes, discovered_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, strftime('%Y-%m-%dT%H:%M:%fZ','now'))
         ON CONFLICT(id) DO UPDATE SET
            native_fsid=excluded.native_fsid, name=excluded.name, health=excluded.health,
            raw_capacity_bytes=excluded.raw_capacity_bytes, used_capacity_bytes=excluded.used_capacity_bytes,
            available_capacity_bytes=excluded.available_capacity_bytes, discovered_at=excluded.discovered_at",
    )
    .bind(&d.cluster.id)
    .bind(backend_id)
    .bind(&d.cluster.native_fsid)
    .bind(&d.cluster.name)
    .bind(health_str(d.cluster.health))
    .bind(d.cluster.raw_capacity_bytes)
    .bind(d.cluster.used_capacity_bytes)
    .bind(d.cluster.available_capacity_bytes)
    .execute(&mut *tx)
    .await?;

    for p in &d.pools {
        sqlx::query(
            "INSERT INTO storage_pools (id, cluster_id, name, kind, device_class, replica_size, used_bytes, max_bytes, health)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
             ON CONFLICT(cluster_id, name) DO UPDATE SET
                kind=excluded.kind, device_class=excluded.device_class, replica_size=excluded.replica_size,
                used_bytes=excluded.used_bytes, max_bytes=excluded.max_bytes, health=excluded.health",
        )
        .bind(&p.id)
        .bind(&p.cluster_id)
        .bind(&p.name)
        .bind(&p.kind)
        .bind(&p.device_class)
        .bind(p.replica_size)
        .bind(p.used_bytes)
        .bind(p.max_bytes)
        .bind(health_str(p.health))
        .execute(&mut *tx)
        .await?;
    }

    for o in &d.osds {
        sqlx::query(
            "INSERT INTO storage_osds (id, cluster_id, osd_num, up, in_cluster, device_class, host, used_bytes, capacity_bytes)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
             ON CONFLICT(cluster_id, osd_num) DO UPDATE SET
                up=excluded.up, in_cluster=excluded.in_cluster, device_class=excluded.device_class,
                host=excluded.host, used_bytes=excluded.used_bytes, capacity_bytes=excluded.capacity_bytes",
        )
        .bind(format!("osd_{}_{}", o.cluster_id, o.id))
        .bind(&o.cluster_id)
        .bind(o.id)
        .bind(o.up as i64)
        .bind(o.in_cluster as i64)
        .bind(&o.device_class)
        .bind(&o.host)
        .bind(o.used_bytes)
        .bind(o.capacity_bytes)
        .execute(&mut *tx)
        .await?;
    }

    // Marker taken before upserting this discovery's volumes: every volume the pass touches gets
    // a newer `updated_at`, so anything left older is stale (deleted from the backend) and can be
    // pruned below.
    let discovery_ts: String = sqlx::query_scalar("SELECT strftime('%Y-%m-%dT%H:%M:%fZ','now')")
        .fetch_one(&mut *tx)
        .await?;

    for v in &d.volumes {
        sqlx::query(
            "INSERT INTO storage_volumes
                (id, backend_id, cluster_id, pool_id, name, kind, backend_native_id, size_bytes, used_bytes,
                 state, health, kubernetes_namespace, pvc_name, storage_class_name, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, strftime('%Y-%m-%dT%H:%M:%fZ','now'))
             ON CONFLICT(id) DO UPDATE SET
                cluster_id=excluded.cluster_id, pool_id=excluded.pool_id, name=excluded.name, kind=excluded.kind,
                backend_native_id=excluded.backend_native_id, size_bytes=excluded.size_bytes, used_bytes=excluded.used_bytes,
                state=excluded.state, health=excluded.health,
                -- Preserve Kubernetes correlation across re-discovery: a driver pass that can't
                -- attribute the RBD image (e.g. the periodic monitor, which has no K8s client)
                -- reports NULL — keep the previously enriched value instead of wiping it.
                kubernetes_namespace=COALESCE(excluded.kubernetes_namespace, storage_volumes.kubernetes_namespace),
                pvc_name=COALESCE(excluded.pvc_name, storage_volumes.pvc_name),
                storage_class_name=COALESCE(excluded.storage_class_name, storage_volumes.storage_class_name),
                updated_at=excluded.updated_at",
        )
        .bind(&v.id)
        .bind(backend_id)
        .bind(&v.cluster_id)
        .bind(&v.pool_id)
        .bind(&v.name)
        .bind(volume_kind_str(v.kind))
        .bind(&v.backend_native_id)
        .bind(v.size_bytes)
        .bind(v.used_bytes)
        .bind(&v.state)
        .bind(health_str(v.health))
        .bind(&v.kubernetes_namespace)
        .bind(&v.pvc_name)
        .bind(&v.storage_class_name)
        .execute(&mut *tx)
        .await?;
    }

    // Prune block volumes this backend no longer reports (deleted from Ceph) so orphaned inventory
    // doesn't linger. Protective: never touch volumes a product owns (product_bindings) or that a
    // snapshot depends on. Self-healing — a transient miss just re-adds the row next discovery.
    let pruned = sqlx::query(
        "DELETE FROM storage_volumes
          WHERE backend_id = ? AND kind = 'block' AND updated_at < ?
            AND id NOT IN (
                SELECT storage_resource_id FROM product_bindings WHERE storage_resource_type = 'volume'
            )
            AND id NOT IN (SELECT volume_id FROM storage_snapshots)",
    )
    .bind(backend_id)
    .bind(&discovery_ts)
    .execute(&mut *tx)
    .await?;
    if pruned.rows_affected() > 0 {
        tracing::info!(
            backend = backend_id,
            pruned = pruned.rows_affected(),
            "discovery.pruned_stale_volumes"
        );
    }

    tx.commit().await?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Reads
// ---------------------------------------------------------------------------

/// Policy drift (day-2 governance): volumes whose applied StorageClass no longer matches the
/// StorageClass their assigned policy resolves to, or whose policy was deleted.
pub async fn list_policy_drift(pool: &SqlitePool) -> Result<Vec<serde_json::Value>> {
    let rows = sqlx::query(
        "SELECT v.id AS volume_id, v.name AS name, v.storage_class_name AS actual, v.policy_id AS policy_id,
                json_extract(p.placement,'$.storage_class') AS expected,
                CASE WHEN p.id IS NULL THEN 1 ELSE 0 END AS policy_missing
         FROM storage_volumes v LEFT JOIN storage_policies p ON p.id = v.policy_id
         WHERE v.policy_id IS NOT NULL",
    )
    .fetch_all(pool)
    .await?;
    let mut drift = Vec::new();
    for r in rows {
        let actual: Option<String> = r.get("actual");
        let expected: Option<String> = r.get("expected");
        let missing: i64 = r.get("policy_missing");
        let drifted = missing == 1 || (expected.is_some() && actual != expected);
        if drifted {
            drift.push(serde_json::json!({
                "volume_id": r.get::<String, _>("volume_id"),
                "name": r.get::<String, _>("name"),
                "policy_id": r.get::<Option<String>, _>("policy_id"),
                "expected_storage_class": expected,
                "actual_storage_class": actual,
                "reason": if missing == 1 { "policy deleted" } else { "storage class differs from policy" },
            }));
        }
    }
    Ok(drift)
}

/// Cordon or uncordon a backend (day-2 maintenance). Returns whether a row changed.
pub async fn set_backend_cordoned(pool: &SqlitePool, id: &str, cordoned: bool) -> Result<bool> {
    let res = sqlx::query(
        "UPDATE storage_backends SET cordoned=?, updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE id=?",
    )
    .bind(cordoned as i64)
    .bind(id)
    .execute(pool)
    .await?;
    Ok(res.rows_affected() > 0)
}

/// Whether a backend is cordoned (rejects new provisioning). Missing backend → not cordoned.
pub async fn is_backend_cordoned(pool: &SqlitePool, id: &str) -> Result<bool> {
    let v: Option<i64> = sqlx::query_scalar("SELECT cordoned FROM storage_backends WHERE id=?")
        .bind(id)
        .fetch_optional(pool)
        .await?;
    Ok(v.unwrap_or(0) != 0)
}

/// Number of volumes still referencing a backend — a guard against deleting an in-use backend.
pub async fn backend_volume_count(pool: &SqlitePool, id: &str) -> Result<i64> {
    Ok(sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM storage_volumes WHERE backend_id=?")
        .bind(id)
        .fetch_one(pool)
        .await
        .unwrap_or(0))
}

/// Delete a backend inventory row (e.g. a decommissioned or fixture backend). Callers should refuse
/// when [`backend_volume_count`] is non-zero so live volumes aren't orphaned.
pub async fn delete_backend(pool: &SqlitePool, id: &str) -> Result<()> {
    sqlx::query("DELETE FROM storage_backends WHERE id=?")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

/// Purge a backend's leftover *inventory* volume rows without touching any real storage — for a
/// decommissioned/fixture backend whose discovered volumes (e.g. NFS exports) have no live driver.
pub async fn delete_volumes_by_backend(pool: &SqlitePool, backend_id: &str) -> Result<u64> {
    let r = sqlx::query("DELETE FROM storage_volumes WHERE backend_id=?")
        .bind(backend_id)
        .execute(pool)
        .await?;
    Ok(r.rows_affected())
}

pub async fn list_backends(pool: &SqlitePool) -> Result<Vec<StorageBackend>> {
    let rows = sqlx::query(
        "SELECT id, name, backend_type, mode, status, capabilities, connection_ref FROM storage_backends ORDER BY name",
    )
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| {
            let caps: Capabilities =
                serde_json::from_str(r.get::<String, _>("capabilities").as_str())
                    .unwrap_or_default();
            StorageBackend {
                id: r.get("id"),
                name: r.get("name"),
                backend_type: backend_type_from(r.get::<String, _>("backend_type").as_str()),
                mode: mode_from(r.get::<String, _>("mode").as_str()),
                status: r.get("status"),
                capabilities: caps,
                connection_ref: r.get("connection_ref"),
            }
        })
        .collect())
}

pub async fn list_clusters(pool: &SqlitePool) -> Result<Vec<StorageCluster>> {
    let rows = sqlx::query(
        "SELECT id, backend_id, native_fsid, name, health, raw_capacity_bytes, used_capacity_bytes, available_capacity_bytes
         FROM storage_clusters ORDER BY name",
    )
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().map(row_to_cluster).collect())
}

pub async fn get_cluster(pool: &SqlitePool, id: &str) -> Result<Option<StorageCluster>> {
    let row = sqlx::query(
        "SELECT id, backend_id, native_fsid, name, health, raw_capacity_bytes, used_capacity_bytes, available_capacity_bytes
         FROM storage_clusters WHERE id = ?",
    )
    .bind(id)
    .fetch_optional(pool)
    .await?;
    Ok(row.map(row_to_cluster))
}

/// Derive a health snapshot for a cluster from its stored row.
pub async fn cluster_health(pool: &SqlitePool, id: &str) -> Result<Option<StorageHealth>> {
    Ok(get_cluster(pool, id).await?.map(|c| StorageHealth {
        status: c.health,
        summary: health_str(c.health).to_uppercase(),
        raw_capacity_bytes: c.raw_capacity_bytes,
        used_capacity_bytes: c.used_capacity_bytes,
        available_capacity_bytes: c.available_capacity_bytes,
        recovering: false,
        degraded_objects: 0,
    }))
}

fn row_to_cluster(r: sqlx::sqlite::SqliteRow) -> StorageCluster {
    StorageCluster {
        id: r.get("id"),
        backend_id: r.get("backend_id"),
        native_fsid: r.get("native_fsid"),
        name: r.get("name"),
        health: health_from(r.get::<String, _>("health").as_str()),
        raw_capacity_bytes: r.get("raw_capacity_bytes"),
        used_capacity_bytes: r.get("used_capacity_bytes"),
        available_capacity_bytes: r.get("available_capacity_bytes"),
    }
}

pub async fn list_pools(pool: &SqlitePool) -> Result<Vec<StoragePool>> {
    list_pools_filtered(pool, None, None).await
}

/// List pools, optionally filtered by owning backend (via the cluster join) and/or pool `kind`.
pub async fn list_pools_filtered(
    pool: &SqlitePool,
    backend_id: Option<&str>,
    kind: Option<&str>,
) -> Result<Vec<StoragePool>> {
    let rows = sqlx::query(
        "SELECT id, cluster_id, name, kind, device_class, replica_size, used_bytes, max_bytes, health
         FROM storage_pools
         WHERE (? IS NULL OR kind = ?)
           AND (? IS NULL OR cluster_id IN (SELECT id FROM storage_clusters WHERE backend_id = ?))
         ORDER BY name",
    )
    .bind(kind)
    .bind(kind)
    .bind(backend_id)
    .bind(backend_id)
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| StoragePool {
            id: r.get("id"),
            cluster_id: r.get("cluster_id"),
            name: r.get("name"),
            kind: r.get("kind"),
            device_class: r.get("device_class"),
            replica_size: r.get("replica_size"),
            used_bytes: r.get("used_bytes"),
            max_bytes: r.get("max_bytes"),
            health: health_from(
                r.get::<Option<String>, _>("health")
                    .unwrap_or_default()
                    .as_str(),
            ),
        })
        .collect())
}

pub async fn list_osds(pool: &SqlitePool) -> Result<Vec<Osd>> {
    let rows = sqlx::query(
        "SELECT cluster_id, osd_num, up, in_cluster, device_class, host, used_bytes, capacity_bytes
         FROM storage_osds ORDER BY cluster_id, osd_num",
    )
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| Osd {
            id: r.get("osd_num"),
            cluster_id: r.get("cluster_id"),
            up: r.get::<i64, _>("up") != 0,
            in_cluster: r.get::<i64, _>("in_cluster") != 0,
            device_class: r.get("device_class"),
            host: r.get("host"),
            used_bytes: r.get("used_bytes"),
            capacity_bytes: r.get("capacity_bytes"),
        })
        .collect())
}

pub async fn list_volumes(pool: &SqlitePool) -> Result<Vec<StorageVolume>> {
    let rows = sqlx::query(&volume_select("ORDER BY name"))
        .fetch_all(pool)
        .await?;
    Ok(rows.into_iter().map(row_to_volume).collect())
}

/// Merge `labels` (a JSON object) into a volume's `metadata.labels`. Returns the merged label map.
pub async fn set_volume_labels(
    pool: &SqlitePool,
    id: &str,
    labels: &serde_json::Map<String, serde_json::Value>,
) -> Result<serde_json::Value> {
    let existing: Option<String> =
        sqlx::query_scalar("SELECT metadata FROM storage_volumes WHERE id = ?")
            .bind(id)
            .fetch_optional(pool)
            .await?;
    let mut meta: serde_json::Value = existing
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_else(|| serde_json::json!({}));
    let map = meta
        .as_object_mut()
        .ok_or_else(|| anyhow::anyhow!("volume metadata is not an object"))?;
    let current = map.entry("labels").or_insert_with(|| serde_json::json!({}));
    if let Some(cur) = current.as_object_mut() {
        for (k, v) in labels {
            cur.insert(k.clone(), v.clone());
        }
    }
    let merged = map.get("labels").cloned().unwrap_or(serde_json::json!({}));
    sqlx::query("UPDATE storage_volumes SET metadata = ?, updated_at = strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE id = ?")
        .bind(meta.to_string())
        .bind(id)
        .execute(pool)
        .await?;
    Ok(merged)
}

/// Read a volume's `metadata.labels` (empty object if none).
pub async fn get_volume_labels(pool: &SqlitePool, id: &str) -> Result<serde_json::Value> {
    let meta: Option<String> =
        sqlx::query_scalar("SELECT metadata FROM storage_volumes WHERE id = ?")
            .bind(id)
            .fetch_optional(pool)
            .await?;
    let v: serde_json::Value = meta
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_else(|| serde_json::json!({}));
    Ok(v.get("labels").cloned().unwrap_or(serde_json::json!({})))
}

/// The owning tenant of a volume (the DTO omits it); defaults to `global` if the volume is gone.
pub async fn volume_tenant(pool: &SqlitePool, id: &str) -> Result<String> {
    let t: Option<String> =
        sqlx::query_scalar("SELECT tenant_id FROM storage_volumes WHERE id = ?")
            .bind(id)
            .fetch_optional(pool)
            .await?;
    Ok(t.unwrap_or_else(|| "global".into()))
}

/// List volumes with optional `state` / `tenant_id` equality filters.
pub async fn list_volumes_filtered(
    pool: &SqlitePool,
    state: Option<&str>,
    tenant_id: Option<&str>,
    backend_id: Option<&str>,
    kind: Option<&str>,
) -> Result<Vec<StorageVolume>> {
    let rows = sqlx::query(&volume_select(
        "WHERE (? IS NULL OR state = ?) AND (? IS NULL OR tenant_id = ?)
           AND (? IS NULL OR backend_id = ?) AND (? IS NULL OR kind = ?) ORDER BY name",
    ))
    .bind(state)
    .bind(state)
    .bind(tenant_id)
    .bind(tenant_id)
    .bind(backend_id)
    .bind(backend_id)
    .bind(kind)
    .bind(kind)
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().map(row_to_volume).collect())
}

pub async fn get_volume(pool: &SqlitePool, id: &str) -> Result<Option<StorageVolume>> {
    let row = sqlx::query(&volume_select("WHERE id = ?"))
        .bind(id)
        .fetch_optional(pool)
        .await?;
    Ok(row.map(row_to_volume))
}

fn volume_select(tail: &str) -> String {
    format!(
        "SELECT id, cluster_id, pool_id, name, kind, backend_native_id, size_bytes, used_bytes, state, health,
                kubernetes_namespace, pvc_name, storage_class_name
         FROM storage_volumes {tail}"
    )
}

/// List volumes owned by a product (via `product_bindings`), optionally scoped to a single owning
/// resource id. Used by the gRPC edge so a product can enumerate only the volumes it owns.
pub async fn list_volumes_by_owner(
    pool: &SqlitePool,
    product: &str,
    resource_id: Option<&str>,
) -> Result<Vec<StorageVolume>> {
    let sql = "SELECT DISTINCT v.id, v.cluster_id, v.pool_id, v.name, v.kind, v.backend_native_id,
                      v.size_bytes, v.used_bytes, v.state, v.health, v.kubernetes_namespace,
                      v.pvc_name, v.storage_class_name
               FROM storage_volumes v
               JOIN product_bindings b
                 ON b.storage_resource_type = 'volume' AND b.storage_resource_id = v.id
               WHERE b.product = ? AND (? IS NULL OR b.resource_id = ?)
               ORDER BY v.name";
    let rows = sqlx::query(sql)
        .bind(product)
        .bind(resource_id)
        .bind(resource_id)
        .fetch_all(pool)
        .await?;
    Ok(rows.into_iter().map(row_to_volume).collect())
}

fn row_to_volume(r: sqlx::sqlite::SqliteRow) -> StorageVolume {
    StorageVolume {
        id: r.get("id"),
        cluster_id: r.get("cluster_id"),
        pool_id: r.get("pool_id"),
        name: r.get("name"),
        kind: volume_kind_from(r.get::<String, _>("kind").as_str()),
        backend_native_id: r.get("backend_native_id"),
        size_bytes: r.get("size_bytes"),
        used_bytes: r.get("used_bytes"),
        state: r.get("state"),
        health: health_from(r.get::<String, _>("health").as_str()),
        kubernetes_namespace: r.get("kubernetes_namespace"),
        pvc_name: r.get("pvc_name"),
        storage_class_name: r.get("storage_class_name"),
    }
}

/// Per-backend inventory breakdown (backend type, cluster/volume counts, capacity) so the
/// multi-backend picture is distinguishable in the API and Prometheus.
pub async fn backend_breakdown(pool: &SqlitePool) -> Result<Vec<serde_json::Value>> {
    let rows = sqlx::query(
        "SELECT b.id AS id, b.backend_type AS backend_type, b.mode AS mode, b.status AS status,
                (SELECT COUNT(*) FROM storage_clusters c WHERE c.backend_id=b.id) AS clusters,
                (SELECT COUNT(*) FROM storage_volumes v WHERE v.backend_id=b.id) AS volumes,
                COALESCE((SELECT SUM(raw_capacity_bytes)  FROM storage_clusters c WHERE c.backend_id=b.id),0) AS raw,
                COALESCE((SELECT SUM(used_capacity_bytes) FROM storage_clusters c WHERE c.backend_id=b.id),0) AS used
         FROM storage_backends b ORDER BY b.id",
    )
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| {
            serde_json::json!({
                "backend_id": r.get::<String, _>("id"),
                "backend_type": r.get::<String, _>("backend_type"),
                "mode": r.get::<String, _>("mode"),
                "status": r.get::<String, _>("status"),
                "clusters": r.get::<i64, _>("clusters"),
                "volumes": r.get::<i64, _>("volumes"),
                "raw_capacity_bytes": r.get::<i64, _>("raw"),
                "used_capacity_bytes": r.get::<i64, _>("used"),
            })
        })
        .collect())
}

/// Aggregate capacity summary across all clusters (PDF §13.2 overview cards).
pub async fn metrics_summary(pool: &SqlitePool) -> Result<serde_json::Value> {
    let row = sqlx::query(
        "SELECT
            COALESCE(SUM(raw_capacity_bytes),0)       AS raw,
            COALESCE(SUM(used_capacity_bytes),0)      AS used,
            COALESCE(SUM(available_capacity_bytes),0) AS avail,
            COUNT(*)                                  AS clusters
         FROM storage_clusters",
    )
    .fetch_one(pool)
    .await?;
    let volumes: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM storage_volumes")
        .fetch_one(pool)
        .await?;
    let pools_n: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM storage_pools")
        .fetch_one(pool)
        .await?;
    let snapshots: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM storage_snapshots")
        .fetch_one(pool)
        .await
        .unwrap_or(0);
    let buckets: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM storage_buckets")
        .fetch_one(pool)
        .await
        .unwrap_or(0);
    let backups: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM storage_backups")
        .fetch_one(pool)
        .await
        .unwrap_or(0);
    // Client I/O + recovery rollups from the latest scraped Ceph metrics (0 if none yet).
    let raw = row.get::<i64, _>("raw");
    let used = row.get::<i64, _>("used");
    let used_pct = if raw > 0 {
        (used as f64 / raw as f64 * 100.0).round() / 100.0
    } else {
        0.0
    };
    async fn sum(pool: &SqlitePool, n: &str) -> f64 {
        metrics::sum_value(pool, n).await.unwrap_or(0.0)
    }
    Ok(serde_json::json!({
        "raw_capacity_bytes": raw,
        "used_capacity_bytes": used,
        "available_capacity_bytes": row.get::<i64, _>("avail"),
        "used_capacity_percent": used_pct,
        "clusters": row.get::<i64, _>("clusters"),
        "pools": pools_n,
        "volumes": volumes,
        "snapshots": snapshots,
        "buckets": buckets,
        "backups": backups,
        "client_io": {
            "read_ops_total": sum(pool, "ceph_pool_rd").await,
            "write_ops_total": sum(pool, "ceph_pool_wr").await,
            "read_bytes_total": sum(pool, "ceph_pool_rd_bytes").await,
            "write_bytes_total": sum(pool, "ceph_pool_wr_bytes").await,
        },
        "recovery": {
            "pg_recovering": sum(pool, "ceph_pg_recovering").await,
            "pg_backfilling": sum(pool, "ceph_pg_backfilling").await,
            "objects_degraded": sum(pool, "ceph_num_objects_degraded").await,
            "objects_misplaced": sum(pool, "ceph_num_objects_misplaced").await,
            "objects_unfound": sum(pool, "ceph_num_objects_unfound").await,
        },
    }))
}
