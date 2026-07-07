// Copyright (c) 2026 ZyvorAI Labs Private Limited. All rights reserved.
//! Snapshot records (PDF §11 storage_snapshots).

use anyhow::Result;
use atlas_api_types::StorageSnapshot;
use sqlx::{Row, SqlitePool};

/// Insert a snapshot row.
#[allow(clippy::too_many_arguments)]
pub async fn insert_snapshot(
    pool: &SqlitePool,
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
         VALUES (?, ?, ?, ?, ?, ?, ?)",
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

pub async fn set_state(pool: &SqlitePool, id: &str, state: &str) -> Result<()> {
    sqlx::query("UPDATE storage_snapshots SET state=? WHERE id=?")
        .bind(state)
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

/// Mark a snapshot protected (has dependent clones) or not.
pub async fn set_protected(pool: &SqlitePool, id: &str, protected: bool) -> Result<()> {
    sqlx::query("UPDATE storage_snapshots SET protected=? WHERE id=?")
        .bind(protected as i64)
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn get_snapshot(pool: &SqlitePool, id: &str) -> Result<Option<StorageSnapshot>> {
    let row = sqlx::query(&select("WHERE id = ?"))
        .bind(id)
        .fetch_optional(pool)
        .await?;
    Ok(row.map(row_to_snapshot))
}

pub async fn list_snapshots(
    pool: &SqlitePool,
    volume_id: Option<&str>,
) -> Result<Vec<StorageSnapshot>> {
    let rows = match volume_id {
        Some(v) => {
            sqlx::query(&select("WHERE volume_id = ? ORDER BY created_at DESC"))
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

pub async fn delete_snapshot_row(pool: &SqlitePool, id: &str) -> Result<()> {
    sqlx::query("DELETE FROM storage_snapshots WHERE id=?")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

fn select(tail: &str) -> String {
    format!(
        "SELECT id, tenant_id, volume_id, name, backend_native_id, consistency, state, protected, parent_snapshot_id, created_at
         FROM storage_snapshots {tail}"
    )
}

fn row_to_snapshot(r: sqlx::sqlite::SqliteRow) -> StorageSnapshot {
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
