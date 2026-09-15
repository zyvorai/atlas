// Copyright (c) 2026 ZyvorAI Labs Private Limited.
// SPDX-License-Identifier: AGPL-3.0-only OR LicenseRef-Atlas-Commercial
//! Audit-log writes (PDF §14.3). Every state-changing or sensitive action should append here.

use anyhow::Result;
use sqlx::{AnyPool, Row};

use crate::now_rfc3339;

/// Append an audit record. `request`/`result` are optional JSON blobs.
#[allow(clippy::too_many_arguments)]
pub async fn record(
    pool: &AnyPool,
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
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
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
    pool: &AnyPool,
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
         WHERE ($1 IS NULL OR actor_id LIKE $2 ESCAPE '\\')
           AND ($3 IS NULL OR action LIKE $4 ESCAPE '\\')
           AND ($5 IS NULL OR resource_type = $6)
           AND ($7 IS NULL OR resource_id = $8)
         ORDER BY id DESC
         LIMIT $9",
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

/// Delete audit rows older than `keep_days` (day-2 retention). Returns how many were pruned.
pub async fn prune(pool: &AnyPool, keep_days: i64) -> Result<u64> {
    let cutoff = now_rfc3339(chrono::Utc::now() - chrono::Duration::days(keep_days.max(1)));
    let res = sqlx::query("DELETE FROM storage_audit_logs WHERE created_at < $1")
        .bind(cutoff)
        .execute(pool)
        .await?;
    Ok(res.rows_affected())
}

/// Rows eligible for retention pruning (same cutoff `prune` uses), oldest first — for a caller
/// that wants to export them to an external sink (SIEM/syslog) before they're deleted.
pub async fn rows_older_than(pool: &AnyPool, keep_days: i64) -> Result<Vec<serde_json::Value>> {
    let cutoff = now_rfc3339(chrono::Utc::now() - chrono::Duration::days(keep_days.max(1)));
    let rows = sqlx::query(
        "SELECT id, tenant_id, actor_id, action, resource_type, resource_id, status,
                request, result, created_at
         FROM storage_audit_logs
         WHERE created_at < $1
         ORDER BY id ASC",
    )
    .bind(cutoff)
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
pub async fn delete_ids(pool: &AnyPool, ids: &[i64]) -> Result<u64> {
    if ids.is_empty() {
        return Ok(0);
    }
    let placeholders = (1..=ids.len())
        .map(|i| format!("${i}"))
        .collect::<Vec<_>>()
        .join(",");
    let sql = format!("DELETE FROM storage_audit_logs WHERE id IN ({placeholders})");
    let mut q = sqlx::query(&sql);
    for id in ids {
        q = q.bind(id);
    }
    let res = q.execute(pool).await?;
    Ok(res.rows_affected())
}

pub async fn count_for_action(pool: &AnyPool, action: &str) -> Result<i64> {
    let n: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM storage_audit_logs WHERE action = $1")
        .bind(action)
        .fetch_one(pool)
        .await?;
    Ok(n)
}
