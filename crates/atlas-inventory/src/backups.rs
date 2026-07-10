// Copyright (c) 2026 ZyvorAI Labs Private Limited. All rights reserved.
//! Backup records + manifests (PDF §16.2, §11 storage_backups).

use anyhow::Result;
use atlas_api_types::BackupRecord;
use sqlx::{Row, SqlitePool};

#[allow(clippy::too_many_arguments)]
pub async fn insert_backup(
    pool: &SqlitePool,
    id: &str,
    tenant_id: &str,
    volume_id: &str,
    snapshot_id: Option<&str>,
    bucket_id: &str,
    object_key: &str,
    format: &str,
    manifest: &serde_json::Value,
) -> Result<()> {
    sqlx::query(
        "INSERT INTO storage_backups (id, tenant_id, volume_id, snapshot_id, bucket_id, object_key, format, state, manifest)
         VALUES (?, ?, ?, ?, ?, ?, ?, 'pending', ?)",
    )
    .bind(id)
    .bind(tenant_id)
    .bind(volume_id)
    .bind(snapshot_id)
    .bind(bucket_id)
    .bind(object_key)
    .bind(format)
    .bind(manifest.to_string())
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn set_state(
    pool: &SqlitePool,
    id: &str,
    state: &str,
    checksum: Option<&str>,
) -> Result<()> {
    sqlx::query("UPDATE storage_backups SET state=?, checksum=COALESCE(?, checksum) WHERE id=?")
        .bind(state)
        .bind(checksum)
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn delete_backup_row(pool: &SqlitePool, id: &str) -> Result<()> {
    sqlx::query("DELETE FROM storage_backups WHERE id=?")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

/// Count backups referencing a bucket (blocks bucket deletion while non-empty).
pub async fn count_for_bucket(pool: &SqlitePool, bucket_id: &str) -> Result<i64> {
    Ok(
        sqlx::query_scalar("SELECT COUNT(*) FROM storage_backups WHERE bucket_id=?")
            .bind(bucket_id)
            .fetch_one(pool)
            .await?,
    )
}

pub async fn get_backup(pool: &SqlitePool, id: &str) -> Result<Option<BackupRecord>> {
    let row = sqlx::query(&select("WHERE id = ?"))
        .bind(id)
        .fetch_optional(pool)
        .await?;
    Ok(row.map(row_to_backup))
}

pub async fn list_backups(pool: &SqlitePool, volume_id: Option<&str>) -> Result<Vec<BackupRecord>> {
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
    Ok(rows.into_iter().map(row_to_backup).collect())
}

/// Backups whose source volume no longer exists — orphans. `storage_backups.volume_id` has no FK
/// (unlike snapshots, which cascade), so deleting a volume leaves its backups dangling in the
/// catalog. Day-2 hygiene surfaces these so an operator can reclaim them.
pub async fn list_orphans(pool: &SqlitePool) -> Result<Vec<BackupRecord>> {
    let rows = sqlx::query(&select(
        "WHERE NOT EXISTS (SELECT 1 FROM storage_volumes v WHERE v.id = storage_backups.volume_id) \
         ORDER BY created_at DESC",
    ))
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().map(row_to_backup).collect())
}

/// List completed backups (`verified`/`completed`) for a volume created strictly before `cutoff`
/// (RFC3339 UTC, DB format `YYYY-MM-DDTHH:MM:SS.mmmZ`), oldest first — used by age-based retention.
pub async fn list_older_than(
    pool: &SqlitePool,
    volume_id: &str,
    cutoff: &str,
) -> Result<Vec<BackupRecord>> {
    let rows = sqlx::query(&select(
        "WHERE volume_id = ? AND state IN ('verified', 'completed') AND created_at < ? \
         ORDER BY created_at ASC",
    ))
    .bind(volume_id)
    .bind(cutoff)
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().map(row_to_backup).collect())
}

fn select(tail: &str) -> String {
    format!(
        "SELECT id, tenant_id, volume_id, snapshot_id, bucket_id, object_key, format, checksum, state, created_at
         FROM storage_backups {tail}"
    )
}

fn row_to_backup(r: sqlx::sqlite::SqliteRow) -> BackupRecord {
    BackupRecord {
        id: r.get("id"),
        tenant_id: r.get("tenant_id"),
        volume_id: r.get("volume_id"),
        snapshot_id: r.get("snapshot_id"),
        bucket_id: r.get("bucket_id"),
        object_key: r.get("object_key"),
        format: r.get("format"),
        checksum: r.get("checksum"),
        state: r.get("state"),
        created_at: r.get("created_at"),
    }
}
