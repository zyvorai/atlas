// Copyright (c) 2026 ZyvorAI Labs Private Limited. All rights reserved.
//! Audit-log writes (PDF §14.3). Every state-changing or sensitive action should append here.

use anyhow::Result;
use sqlx::{Row, SqlitePool};

/// Append an audit record. `request`/`result` are optional JSON blobs.
#[allow(clippy::too_many_arguments)]
pub async fn record(
    pool: &SqlitePool,
    tenant_id: Option<&str>,
    actor_id: &str,
    action: &str,
    resource_type: &str,
    resource_id: &str,
    status: &str,
    request: Option<serde_json::Value>,
    result: Option<serde_json::Value>,
) -> Result<()> {
    sqlx::query(
        "INSERT INTO storage_audit_logs
            (tenant_id, actor_id, action, resource_type, resource_id, status, request, result)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(tenant_id)
    .bind(actor_id)
    .bind(action)
    .bind(resource_type)
    .bind(resource_id)
    .bind(status)
    .bind(request.map(|v| v.to_string()))
    .bind(result.map(|v| v.to_string()))
    .execute(pool)
    .await?;
    Ok(())
}

/// Query the audit trail, newest first, with a bounded limit. `actor`/`action` are matched as
/// case-insensitive substrings (the UI exposes them as free-text search boxes); `resource_type`/
/// `resource_id` stay exact-match (used for programmatic drill-down, not fuzzy search).
pub async fn list(
    pool: &SqlitePool,
    actor: Option<&str>,
    action: Option<&str>,
    resource_type: Option<&str>,
    resource_id: Option<&str>,
    limit: i64,
) -> Result<Vec<serde_json::Value>> {
    let limit = limit.clamp(1, 1000);
    let actor_like = actor.map(|s| format!("%{}%", s.replace('%', "\\%").replace('_', "\\_")));
    let action_like = action.map(|s| format!("%{}%", s.replace('%', "\\%").replace('_', "\\_")));
    let rows = sqlx::query(
        "SELECT id, tenant_id, actor_id, action, resource_type, resource_id, status,
                request, result, created_at
         FROM storage_audit_logs
         WHERE (? IS NULL OR actor_id LIKE ? ESCAPE '\\')
           AND (? IS NULL OR action LIKE ? ESCAPE '\\')
           AND (? IS NULL OR resource_type = ?)
           AND (? IS NULL OR resource_id = ?)
         ORDER BY id DESC
         LIMIT ?",
    )
    .bind(&actor_like)
    .bind(&actor_like)
    .bind(&action_like)
    .bind(&action_like)
    .bind(resource_type)
    .bind(resource_type)
    .bind(resource_id)
    .bind(resource_id)
    .bind(limit)
    .fetch_all(pool)
    .await?;

    let parse = |s: Option<String>| -> serde_json::Value {
        s.and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or(serde_json::Value::Null)
    };
    Ok(rows
        .into_iter()
        .map(|r| {
            serde_json::json!({
                "id": r.get::<i64, _>("id"),
                "tenant_id": r.get::<Option<String>, _>("tenant_id"),
                "actor_id": r.get::<String, _>("actor_id"),
                "action": r.get::<String, _>("action"),
                "resource_type": r.get::<String, _>("resource_type"),
                "resource_id": r.get::<String, _>("resource_id"),
                "status": r.get::<String, _>("status"),
                "request": parse(r.get::<Option<String>, _>("request")),
                "result": parse(r.get::<Option<String>, _>("result")),
                "created_at": r.get::<String, _>("created_at"),
            })
        })
        .collect())
}

/// Count audit rows for a given action — handy for tests.
/// Delete audit rows older than `keep_days` (day-2 retention). Returns how many were pruned.
pub async fn prune(pool: &SqlitePool, keep_days: i64) -> Result<u64> {
    let res = sqlx::query(
        "DELETE FROM storage_audit_logs WHERE created_at < strftime('%Y-%m-%dT%H:%M:%fZ','now', ?)",
    )
    .bind(format!("-{} days", keep_days.max(1)))
    .execute(pool)
    .await?;
    Ok(res.rows_affected())
}

/// Rows eligible for retention pruning (same cutoff `prune` uses), oldest first — for a caller
/// that wants to export them to an external sink (SIEM/syslog) before they're deleted.
pub async fn rows_older_than(pool: &SqlitePool, keep_days: i64) -> Result<Vec<serde_json::Value>> {
    let rows = sqlx::query(
        "SELECT id, tenant_id, actor_id, action, resource_type, resource_id, status,
                request, result, created_at
         FROM storage_audit_logs
         WHERE created_at < strftime('%Y-%m-%dT%H:%M:%fZ','now', ?)
         ORDER BY id ASC",
    )
    .bind(format!("-{} days", keep_days.max(1)))
    .fetch_all(pool)
    .await?;
    let parse = |s: Option<String>| -> serde_json::Value {
        s.and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or(serde_json::Value::Null)
    };
    Ok(rows
        .into_iter()
        .map(|r| {
            serde_json::json!({
                "id": r.get::<i64, _>("id"),
                "tenant_id": r.get::<Option<String>, _>("tenant_id"),
                "actor_id": r.get::<String, _>("actor_id"),
                "action": r.get::<String, _>("action"),
                "resource_type": r.get::<String, _>("resource_type"),
                "resource_id": r.get::<String, _>("resource_id"),
                "status": r.get::<String, _>("status"),
                "request": parse(r.get::<Option<String>, _>("request")),
                "result": parse(r.get::<Option<String>, _>("result")),
                "created_at": r.get::<String, _>("created_at"),
            })
        })
        .collect())
}

/// Delete specific audit rows by id — used after a successful external export, so nothing is
/// lost if the export sink is unreachable (unlike `prune`, which deletes unconditionally).
pub async fn delete_ids(pool: &SqlitePool, ids: &[i64]) -> Result<u64> {
    if ids.is_empty() {
        return Ok(0);
    }
    let placeholders = ids.iter().map(|_| "?").collect::<Vec<_>>().join(",");
    let sql = format!("DELETE FROM storage_audit_logs WHERE id IN ({placeholders})");
    let mut q = sqlx::query(&sql);
    for id in ids {
        q = q.bind(id);
    }
    let res = q.execute(pool).await?;
    Ok(res.rows_affected())
}

pub async fn count_for_action(pool: &SqlitePool, action: &str) -> Result<i64> {
    let n: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM storage_audit_logs WHERE action = ?")
        .bind(action)
        .fetch_one(pool)
        .await?;
    Ok(n)
}
