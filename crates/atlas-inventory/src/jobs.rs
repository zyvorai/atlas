// Copyright (c) 2026 ZyvorAI Labs Private Limited. All rights reserved.
//! Async job records + state-machine transitions (PDF §10.5, §11 storage_jobs).

use anyhow::Result;
use atlas_api_types::JobRecord;
use sqlx::{Row, SqlitePool};

/// Insert a new job in the `pending` state. Returns the job id.
pub async fn insert_job(
    pool: &SqlitePool,
    id: &str,
    tenant_id: &str,
    job_type: &str,
    requested_by: &str,
    request: &serde_json::Value,
    idempotency_key: Option<&str>,
) -> Result<()> {
    sqlx::query(
        "INSERT INTO storage_jobs (id, tenant_id, job_type, state, requested_by, request, idempotency_key)
         VALUES (?, ?, ?, 'pending', ?, ?, ?)",
    )
    .bind(id)
    .bind(tenant_id)
    .bind(job_type)
    .bind(requested_by)
    .bind(request.to_string())
    .bind(idempotency_key)
    .execute(pool)
    .await?;
    Ok(())
}

/// Return an existing job for an idempotency key, if any (PDF §17.4).
pub async fn find_by_idempotency(pool: &SqlitePool, key: &str) -> Result<Option<JobRecord>> {
    let row = sqlx::query(&job_select(
        "WHERE idempotency_key = ? ORDER BY created_at DESC LIMIT 1",
    ))
    .bind(key)
    .fetch_optional(pool)
    .await?;
    Ok(row.map(row_to_job))
}

pub async fn get_job(pool: &SqlitePool, id: &str) -> Result<Option<JobRecord>> {
    let row = sqlx::query(&job_select("WHERE id = ?"))
        .bind(id)
        .fetch_optional(pool)
        .await?;
    Ok(row.map(row_to_job))
}

pub async fn list_jobs(pool: &SqlitePool, limit: i64) -> Result<Vec<JobRecord>> {
    let rows = sqlx::query(&job_select("ORDER BY created_at DESC LIMIT ?"))
        .bind(limit)
        .fetch_all(pool)
        .await?;
    Ok(rows.into_iter().map(row_to_job).collect())
}

/// Move a job to `running` and stamp `started_at`.
pub async fn mark_running(pool: &SqlitePool, id: &str) -> Result<()> {
    sqlx::query(
        "UPDATE storage_jobs SET state='running', progress_percent=MAX(progress_percent,5),
         started_at=COALESCE(started_at, strftime('%Y-%m-%dT%H:%M:%fZ','now')),
         updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE id=?",
    )
    .bind(id)
    .execute(pool)
    .await?;
    Ok(())
}

/// Update state + progress mid-flight (e.g. `verifying`, 50%).
pub async fn set_state(pool: &SqlitePool, id: &str, state: &str, progress: i64) -> Result<()> {
    sqlx::query(
        "UPDATE storage_jobs SET state=?, progress_percent=?, updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE id=?",
    )
    .bind(state)
    .bind(progress)
    .bind(id)
    .execute(pool)
    .await?;
    Ok(())
}

/// Terminal success: state `succeeded`, 100%, store result.
pub async fn mark_succeeded(pool: &SqlitePool, id: &str, result: &serde_json::Value) -> Result<()> {
    sqlx::query(
        "UPDATE storage_jobs SET state='succeeded', progress_percent=100, result=?,
         completed_at=strftime('%Y-%m-%dT%H:%M:%fZ','now'),
         updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE id=?",
    )
    .bind(result.to_string())
    .bind(id)
    .execute(pool)
    .await?;
    Ok(())
}

/// Terminal failure: state `failed`, store error message.
pub async fn mark_failed(pool: &SqlitePool, id: &str, error: &str) -> Result<()> {
    sqlx::query(
        "UPDATE storage_jobs SET state='failed', error=?,
         completed_at=strftime('%Y-%m-%dT%H:%M:%fZ','now'),
         updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE id=?",
    )
    .bind(error)
    .bind(id)
    .execute(pool)
    .await?;
    Ok(())
}

fn job_select(tail: &str) -> String {
    format!(
        "SELECT id, tenant_id, job_type, state, requested_by, progress_percent, error, result, created_at, updated_at
         FROM storage_jobs {tail}"
    )
}

fn row_to_job(r: sqlx::sqlite::SqliteRow) -> JobRecord {
    let result: serde_json::Value = serde_json::from_str(
        r.get::<Option<String>, _>("result")
            .unwrap_or_else(|| "{}".into())
            .as_str(),
    )
    .unwrap_or(serde_json::Value::Null);
    JobRecord {
        id: r.get("id"),
        tenant_id: r.get("tenant_id"),
        job_type: r.get("job_type"),
        state: r.get("state"),
        requested_by: r.get("requested_by"),
        progress_percent: r.get("progress_percent"),
        error: r.get("error"),
        result,
        created_at: r.get("created_at"),
        updated_at: r.get("updated_at"),
    }
}
