// Copyright (c) 2026 ZyvorAI Labs Private Limited. All rights reserved.
//! Protection schedules: periodic snapshot policy per volume (PDF §12.3). The scheduler worker
//! (in `atlas-jobs`) reads due schedules, enqueues snapshot jobs, and advances `next_run_at`.

use anyhow::Result;
use atlas_api_types::SnapshotSchedule;
use sqlx::{Row, SqlitePool};

fn select(tail: &str) -> String {
    format!(
        "SELECT id, tenant_id, volume_id, kind, bucket_id, mode, interval_secs, keep, enabled,
                last_run_at, next_run_at, created_at
         FROM snapshot_schedules {tail}"
    )
}

fn row_to_schedule(r: sqlx::sqlite::SqliteRow) -> SnapshotSchedule {
    SnapshotSchedule {
        id: r.get("id"),
        tenant_id: r.get("tenant_id"),
        volume_id: r.get("volume_id"),
        kind: r.get("kind"),
        bucket_id: r.get("bucket_id"),
        mode: r.get("mode"),
        interval_secs: r.get("interval_secs"),
        keep: r.get("keep"),
        enabled: r.get::<i64, _>("enabled") != 0,
        last_run_at: r.get("last_run_at"),
        next_run_at: r.get("next_run_at"),
        created_at: r.get("created_at"),
    }
}

/// Create a schedule. `next_run_at` starts one interval from now so the first run is not immediate.
/// `kind` is "snapshot" or "backup"; backups also carry a `bucket_id` + `mode`. Returns the row.
#[allow(clippy::too_many_arguments)]
pub async fn insert(
    pool: &SqlitePool,
    id: &str,
    tenant_id: &str,
    volume_id: &str,
    kind: &str,
    bucket_id: Option<&str>,
    mode: &str,
    interval_secs: i64,
    keep: i64,
) -> Result<SnapshotSchedule> {
    let next = format!("+{interval_secs} seconds");
    sqlx::query(
        "INSERT INTO snapshot_schedules
            (id, tenant_id, volume_id, kind, bucket_id, mode, interval_secs, keep, next_run_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, strftime('%Y-%m-%dT%H:%M:%fZ','now', ?))",
    )
    .bind(id)
    .bind(tenant_id)
    .bind(volume_id)
    .bind(kind)
    .bind(bucket_id)
    .bind(mode)
    .bind(interval_secs)
    .bind(keep)
    .bind(&next)
    .execute(pool)
    .await?;
    Ok(get(pool, id).await?.expect("schedule just inserted"))
}

pub async fn get(pool: &SqlitePool, id: &str) -> Result<Option<SnapshotSchedule>> {
    let row = sqlx::query(&select("WHERE id = ?"))
        .bind(id)
        .fetch_optional(pool)
        .await?;
    Ok(row.map(row_to_schedule))
}

pub async fn list(pool: &SqlitePool, volume_id: Option<&str>) -> Result<Vec<SnapshotSchedule>> {
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
    Ok(rows.into_iter().map(row_to_schedule).collect())
}

pub async fn delete(pool: &SqlitePool, id: &str) -> Result<bool> {
    let res = sqlx::query("DELETE FROM snapshot_schedules WHERE id = ?")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(res.rows_affected() > 0)
}

/// Enabled schedules whose `next_run_at` is in the past (i.e. due to run now).
pub async fn due(pool: &SqlitePool) -> Result<Vec<SnapshotSchedule>> {
    let rows = sqlx::query(&select(
        "WHERE enabled = 1 AND next_run_at <= strftime('%Y-%m-%dT%H:%M:%fZ','now') \
         ORDER BY next_run_at ASC",
    ))
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().map(row_to_schedule).collect())
}

/// Record that a schedule just ran: set `last_run_at = now`, advance `next_run_at` by its interval.
pub async fn mark_ran(pool: &SqlitePool, id: &str) -> Result<()> {
    sqlx::query(
        "UPDATE snapshot_schedules
         SET last_run_at = strftime('%Y-%m-%dT%H:%M:%fZ','now'),
             next_run_at = strftime('%Y-%m-%dT%H:%M:%fZ','now', '+' || interval_secs || ' seconds')
         WHERE id = ?",
    )
    .bind(id)
    .execute(pool)
    .await?;
    Ok(())
}
