// Copyright (c) 2026 ZyvorAI Labs Private Limited. All rights reserved.
//! Cutover records (freeze source, drain CDC lag, switch endpoint, rollback window).

use anyhow::Result;
use atlas_api_types::Cutover;
use sqlx::{Row, SqlitePool};

#[allow(clippy::too_many_arguments)]
pub async fn insert_cutover(
    pool: &SqlitePool,
    id: &str,
    tenant_id: &str,
    plan_id: &str,
    from_endpoint: Option<&str>,
    to_endpoint: Option<&str>,
    drain_deadline: Option<&str>,
    rollback_deadline: Option<&str>,
) -> Result<()> {
    sqlx::query(
        "INSERT INTO cutovers
         (id, tenant_id, plan_id, from_endpoint, to_endpoint, drain_deadline, rollback_deadline, state)
         VALUES (?, ?, ?, ?, ?, ?, ?, 'freezing')",
    )
    .bind(id)
    .bind(tenant_id)
    .bind(plan_id)
    .bind(from_endpoint)
    .bind(to_endpoint)
    .bind(drain_deadline)
    .bind(rollback_deadline)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn set_state(pool: &SqlitePool, id: &str, state: &str) -> Result<()> {
    sqlx::query("UPDATE cutovers SET state=? WHERE id=?")
        .bind(state)
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn set_complete(pool: &SqlitePool, id: &str, state: &str) -> Result<()> {
    sqlx::query(
        "UPDATE cutovers SET state=?, completed_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE id=?",
    )
    .bind(state)
    .bind(id)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn get_cutover(pool: &SqlitePool, id: &str) -> Result<Option<Cutover>> {
    let row = sqlx::query(&select("WHERE id = ?"))
        .bind(id)
        .fetch_optional(pool)
        .await?;
    Ok(row.map(row_to_cutover))
}

pub async fn list_cutovers(pool: &SqlitePool) -> Result<Vec<Cutover>> {
    let rows = sqlx::query(&select("ORDER BY created_at DESC"))
        .fetch_all(pool)
        .await?;
    Ok(rows.into_iter().map(row_to_cutover).collect())
}

pub async fn latest_for_plan(pool: &SqlitePool, plan_id: &str) -> Result<Option<Cutover>> {
    let row = sqlx::query(&select("WHERE plan_id = ? ORDER BY created_at DESC LIMIT 1"))
        .bind(plan_id)
        .fetch_optional(pool)
        .await?;
    Ok(row.map(row_to_cutover))
}

fn select(tail: &str) -> String {
    format!(
        "SELECT id, tenant_id, plan_id, state, from_endpoint, to_endpoint, drain_deadline,
                rollback_deadline, created_at, completed_at
         FROM cutovers {tail}"
    )
}

fn row_to_cutover(r: sqlx::sqlite::SqliteRow) -> Cutover {
    Cutover {
        id: r.get("id"),
        tenant_id: r.get("tenant_id"),
        plan_id: r.get("plan_id"),
        state: r.get("state"),
        from_endpoint: r.get("from_endpoint"),
        to_endpoint: r.get("to_endpoint"),
        drain_deadline: r.get("drain_deadline"),
        rollback_deadline: r.get("rollback_deadline"),
        created_at: r.get("created_at"),
        completed_at: r.get("completed_at"),
    }
}
