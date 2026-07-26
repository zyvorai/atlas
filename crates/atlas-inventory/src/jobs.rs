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
/// Count jobs grouped by state (for the Prometheus self-metrics endpoint).
pub async fn count_by_state(pool: &SqlitePool) -> Result<Vec<(String, i64)>> {
    let rows = sqlx::query("SELECT state, COUNT(*) AS n FROM storage_jobs GROUP BY state")
        .fetch_all(pool)
        .await?;
    Ok(rows
        .into_iter()
        .map(|r| (r.get::<String, _>("state"), r.get::<i64, _>("n")))
        .collect())
}

pub async fn mark_running(pool: &SqlitePool, id: &str) -> Result<()> {
    sqlx::query(
        "UPDATE storage_jobs SET state='running', progress_percent=MAX(progress_percent,5),
         started_at=COALESCE(started_at, strftime('%Y-%m-%dT%H:%M:%fZ','now')),
         locked_at=strftime('%Y-%m-%dT%H:%M:%fZ','now'),
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
         locked_by=NULL, locked_at=NULL, error=NULL,
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
        "UPDATE storage_jobs SET state='failed', error=?, locked_by=NULL, locked_at=NULL,
         completed_at=strftime('%Y-%m-%dT%H:%M:%fZ','now'),
         updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE id=?",
    )
    .bind(error)
    .bind(id)
    .execute(pool)
    .await?;
    Ok(())
}

/// Job ids currently in any of `states`, oldest first — used by boot recovery to re-enqueue work the
/// in-memory channel lost across a restart.
pub async fn ids_by_states(pool: &SqlitePool, states: &[&str]) -> Result<Vec<String>> {
    if states.is_empty() {
        return Ok(vec![]);
    }
    let placeholders = states.iter().map(|_| "?").collect::<Vec<_>>().join(",");
    let sql =
        format!("SELECT id FROM storage_jobs WHERE state IN ({placeholders}) ORDER BY created_at ASC");
    let mut q = sqlx::query(&sql);
    for s in states {
        q = q.bind(*s);
    }
    let rows = q.fetch_all(pool).await?;
    Ok(rows.into_iter().map(|r| r.get::<String, _>("id")).collect())
}

/// Due `queued`/`pending` job ids whose `next_attempt_at` is null or already reached. Honors retry
/// backoff so a boot/poller does not fire a job early. Oldest first, capped at `limit`.
pub async fn due_ids(pool: &SqlitePool, limit: i64) -> Result<Vec<String>> {
    let rows = sqlx::query(
        "SELECT id FROM storage_jobs
         WHERE state IN ('queued', 'pending')
           AND (next_attempt_at IS NULL
                OR next_attempt_at <= strftime('%Y-%m-%dT%H:%M:%fZ','now'))
         ORDER BY created_at ASC
         LIMIT ?",
    )
    .bind(limit.max(1))
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().map(|r| r.get::<String, _>("id")).collect())
}

/// Atomically claim a due job for `worker_id`: transitions `queued`/`pending` → `running` and
/// stamps `locked_by`/`locked_at`. Returns `true` if this caller won the claim (so two workers /
/// a channel wake + poller race cannot double-execute).
pub async fn try_claim(pool: &SqlitePool, id: &str, worker_id: &str) -> Result<bool> {
    let res = sqlx::query(
        "UPDATE storage_jobs SET state='running', progress_percent=MAX(progress_percent,5),
         locked_by=?, locked_at=strftime('%Y-%m-%dT%H:%M:%fZ','now'),
         started_at=COALESCE(started_at, strftime('%Y-%m-%dT%H:%M:%fZ','now')),
         updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now')
         WHERE id=? AND state IN ('queued', 'pending')
           AND (next_attempt_at IS NULL
                OR next_attempt_at <= strftime('%Y-%m-%dT%H:%M:%fZ','now'))",
    )
    .bind(worker_id)
    .bind(id)
    .execute(pool)
    .await?;
    Ok(res.rows_affected() == 1)
}

/// Re-queue `running` jobs whose lock/update is older than `stale_secs` (worker died without boot
/// recovery). Clears the lock and stamps an error note. Returns how many were reclaimed.
pub async fn reclaim_stale_running(pool: &SqlitePool, stale_secs: i64) -> Result<u64> {
    if stale_secs <= 0 {
        return Ok(0);
    }
    let res = sqlx::query(
        "UPDATE storage_jobs SET state='queued', locked_by=NULL, locked_at=NULL,
         error=COALESCE(error, 'reclaimed: stale running lock'),
         next_attempt_at=NULL,
         updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now')
         WHERE state='running'
           AND MAX(COALESCE(locked_at, updated_at, started_at, created_at), updated_at)
               < strftime('%Y-%m-%dT%H:%M:%fZ','now', ?)",
    )
    .bind(format!("-{} seconds", stale_secs))
    .execute(pool)
    .await?;
    Ok(res.rows_affected())
}

/// Fail every job stuck in `running` (a crash/restart left it mid-flight; the side effects may be
/// partial, so we fail-safe rather than blindly re-run). Returns how many were reset.
pub async fn fail_running(pool: &SqlitePool, error: &str) -> Result<u64> {
    let res = sqlx::query(
        "UPDATE storage_jobs SET state='failed', error=?, locked_by=NULL, locked_at=NULL,
         completed_at=strftime('%Y-%m-%dT%H:%M:%fZ','now'),
         updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE state='running'",
    )
    .bind(error)
    .execute(pool)
    .await?;
    Ok(res.rows_affected())
}

/// A job's retry accounting: `(retry_count, max_retries)`.
pub async fn retry_budget(pool: &SqlitePool, id: &str) -> Result<(i64, i64)> {
    let row = sqlx::query("SELECT retry_count, max_retries FROM storage_jobs WHERE id=?")
        .bind(id)
        .fetch_one(pool)
        .await?;
    Ok((row.get("retry_count"), row.get("max_retries")))
}

/// Set a job's retry budget (called at enqueue for retryable job types).
pub async fn set_max_retries(pool: &SqlitePool, id: &str, max_retries: i64) -> Result<()> {
    sqlx::query("UPDATE storage_jobs SET max_retries=? WHERE id=?")
        .bind(max_retries)
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

/// Record a retry: bump the count, requeue the job, and stamp when the next attempt is due.
/// `delay_modifier` is a SQLite datetime modifier applied to `now` (e.g. `"+4 seconds"`).
pub async fn bump_retry(pool: &SqlitePool, id: &str, delay_modifier: &str) -> Result<()> {
    sqlx::query(
        "UPDATE storage_jobs SET state='queued', retry_count=retry_count+1,
         locked_by=NULL, locked_at=NULL,
         next_attempt_at=strftime('%Y-%m-%dT%H:%M:%fZ','now', ?),
         error=NULL, updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE id=?",
    )
    .bind(delay_modifier)
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
