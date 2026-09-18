// Copyright (c) 2026 ZyvorAI Labs Private Limited.
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-1.0
//! Snapshot records (PDF §11 storage_snapshots).

use anyhow::Result;
use atlas_api_types::StorageSnapshot;
use sqlx::{AnyPool, Row};

/// Insert a snapshot row.
#[allow(clippy::too_many_arguments)]
pub async fn insert_snapshot(
    pool: &AnyPool,
    id: &str,
    tenant_id: &str,
    volume_id: &str,
    name: &str,
    backend_native_id: Option<&str>,
    consistency: &str,
    state: &str,
) -> Result<()> {
    sqlx::query(
        "INSERT INTO storage_snapshots (id, tenant_id, volume_id, name, backend_native_id, consistency, state)
         VALUES ($1, $2, $3, $4, $5, $6, $7)",
    )
    .bind(id)
    .bind(tenant_id)
    .bind(volume_id)
    .bind(name)
    .bind(backend_native_id)
    .bind(consistency)
    .bind(state)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn set_state(pool: &AnyPool, id: &str, state: &str) -> Result<()> {
    sqlx::query("UPDATE storage_snapshots SET state=$1 WHERE id=$2")
        .bind(state)
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

/// Mark a snapshot protected (has dependent clones) or not.
pub async fn set_protected(pool: &AnyPool, id: &str, protected: bool) -> Result<()> {
    sqlx::query("UPDATE storage_snapshots SET protected=$1 WHERE id=$2")
        .bind(protected as i64)
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn get_snapshot(pool: &AnyPool, id: &str) -> Result<Option<StorageSnapshot>> {
    let row = sqlx::query(&select("WHERE id = $1"))
        .bind(id)
        .fetch_optional(pool)
        .await?;
    Ok(row.map(row_to_snapshot))
}

pub async fn list_snapshots(
    pool: &AnyPool,
    volume_id: Option<&str>,
) -> Result<Vec<StorageSnapshot>> {
    let rows = match volume_id {
        Some(v) => {
            sqlx::query(&select("WHERE volume_id = $1 ORDER BY created_at DESC"))
                .bind(v)
                .fetch_all(pool)
                .await?
        }
        None => {
            sqlx::query(&select("ORDER BY created_at DESC"))
                .fetch_all(pool)
                .await?
        }
    };
    Ok(rows.into_iter().map(row_to_snapshot).collect())
}

/// Snapshots in a given state — used to reconcile ones the create job's bounded bind-poll gave up
/// on (so they'd otherwise show "creating" forever even once the underlying VolumeSnapshot binds).
pub async fn list_by_state(pool: &AnyPool, state: &str) -> Result<Vec<StorageSnapshot>> {
    let rows = sqlx::query(&select("WHERE state = $1 ORDER BY created_at DESC"))
        .bind(state)
        .fetch_all(pool)
        .await?;
    Ok(rows.into_iter().map(row_to_snapshot).collect())
}

pub async fn delete_snapshot_row(pool: &AnyPool, id: &str) -> Result<()> {
    sqlx::query("DELETE FROM storage_snapshots WHERE id=$1")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

/// The most recent snapshot per volume, in one query — used by `protection::list_protection_status`
/// so a bank-scale fleet doesn't pay one query per volume.
pub async fn latest_by_volume(
    pool: &AnyPool,
) -> Result<std::collections::HashMap<String, StorageSnapshot>> {
    let rows = sqlx::query(
        "SELECT s.id, s.tenant_id, s.volume_id, s.name, s.backend_native_id, s.consistency, \
                s.state, s.protected, s.parent_snapshot_id, s.created_at \
         FROM storage_snapshots s \
         INNER JOIN (SELECT volume_id, MAX(created_at) AS max_created FROM storage_snapshots GROUP BY volume_id) latest \
           ON latest.volume_id = s.volume_id AND latest.max_created = s.created_at",
    )
    .fetch_all(pool)
    .await?;
    let mut out = std::collections::HashMap::new();
    for r in rows {
        let snap = row_to_snapshot(r);
        out.entry(snap.volume_id.clone()).or_insert(snap);
    }
    Ok(out)
}

fn select(tail: &str) -> String {
    format!(
        "SELECT id, tenant_id, volume_id, name, backend_native_id, consistency, state, protected, parent_snapshot_id, created_at
         FROM storage_snapshots {tail}"
    )
}

fn row_to_snapshot(r: sqlx::any::AnyRow) -> StorageSnapshot {
    StorageSnapshot {
        id: r.get("id"),
        tenant_id: r.get("tenant_id"),
        volume_id: r.get("volume_id"),
        name: r.get("name"),
        backend_native_id: r.get("backend_native_id"),
        consistency: r.get("consistency"),
        state: r.get("state"),
        protected: r.get::<i64, _>("protected") != 0,
        parent_snapshot_id: r.get("parent_snapshot_id"),
        created_at: r.get("created_at"),
    }
}
