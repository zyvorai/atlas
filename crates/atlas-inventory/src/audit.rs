// Copyright (c) 2026 ZyvorAI Labs Private Limited. All rights reserved.
//! Audit-log writes (PDF §14.3). Every state-changing or sensitive action should append here.

use anyhow::Result;
use sqlx::SqlitePool;

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

/// Count audit rows for a given action — handy for tests.
pub async fn count_for_action(pool: &SqlitePool, action: &str) -> Result<i64> {
    let n: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM storage_audit_logs WHERE action = ?")
        .bind(action)
        .fetch_one(pool)
        .await?;
    Ok(n)
}
