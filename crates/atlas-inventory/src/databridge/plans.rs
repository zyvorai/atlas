// Copyright (c) 2026 ZyvorAI Labs Private Limited. All rights reserved.
//! Migration plan records (source -> edge pipeline state).

use anyhow::Result;
use atlas_api_types::MigrationPlan;
use sqlx::{Row, SqlitePool};

pub async fn insert_plan(
    pool: &SqlitePool,
    id: &str,
    tenant_id: &str,
    name: &str,
    source_id: &str,
    rollback_window_secs: i64,
) -> Result<()> {
    sqlx::query(
        "INSERT INTO migration_plans (id, tenant_id, name, source_id, rollback_window_secs, state)
         VALUES (?, ?, ?, ?, ?, 'draft')",
    )
    .bind(id)
    .bind(tenant_id)
    .bind(name)
    .bind(source_id)
    .bind(rollback_window_secs)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn set_state(pool: &SqlitePool, id: &str, state: &str) -> Result<()> {
    sqlx::query("UPDATE migration_plans SET state=? WHERE id=?")
        .bind(state)
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn set_assessment(
    pool: &SqlitePool,
    id: &str,
    readiness_score: i64,
    assessment: &serde_json::Value,
) -> Result<()> {
    sqlx::query(
        "UPDATE migration_plans SET readiness_score=?, assessment=?, state='assessed' WHERE id=?",
    )
    .bind(readiness_score)
    .bind(assessment.to_string())
    .bind(id)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn set_edge_cluster(pool: &SqlitePool, id: &str, edge_cluster_id: &str) -> Result<()> {
    sqlx::query("UPDATE migration_plans SET edge_cluster_id=? WHERE id=?")
        .bind(edge_cluster_id)
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn set_cdc_stream(pool: &SqlitePool, id: &str, cdc_stream_id: &str) -> Result<()> {
    sqlx::query("UPDATE migration_plans SET cdc_stream_id=? WHERE id=?")
        .bind(cdc_stream_id)
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn delete_plan_row(pool: &SqlitePool, id: &str) -> Result<()> {
    sqlx::query("DELETE FROM migration_plans WHERE id=?")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn get_plan(pool: &SqlitePool, id: &str) -> Result<Option<MigrationPlan>> {
    let row = sqlx::query(&select("WHERE id = ?"))
        .bind(id)
        .fetch_optional(pool)
        .await?;
    Ok(row.map(row_to_plan))
}

pub async fn list_plans(pool: &SqlitePool) -> Result<Vec<MigrationPlan>> {
    let rows = sqlx::query(&select("ORDER BY created_at DESC"))
        .fetch_all(pool)
        .await?;
    Ok(rows.into_iter().map(row_to_plan).collect())
}

/// Plans in a given pipeline state — used by the reconciler to advance in-flight work.
pub async fn list_by_state(pool: &SqlitePool, state: &str) -> Result<Vec<MigrationPlan>> {
    let rows = sqlx::query(&select("WHERE state = ? ORDER BY created_at DESC"))
        .bind(state)
        .fetch_all(pool)
        .await?;
    Ok(rows.into_iter().map(row_to_plan).collect())
}

fn select(tail: &str) -> String {
    format!(
        "SELECT id, tenant_id, name, source_id, edge_cluster_id, cdc_stream_id, readiness_score,
                assessment, rollback_window_secs, cutover_at, state, created_at
         FROM migration_plans {tail}"
    )
}

fn row_to_plan(r: sqlx::sqlite::SqliteRow) -> MigrationPlan {
    let assessment: String = r.get("assessment");
    MigrationPlan {
        id: r.get("id"),
        tenant_id: r.get("tenant_id"),
        name: r.get("name"),
        source_id: r.get("source_id"),
        edge_cluster_id: r.get("edge_cluster_id"),
        cdc_stream_id: r.get("cdc_stream_id"),
        readiness_score: r.get("readiness_score"),
        assessment: serde_json::from_str(&assessment).unwrap_or(serde_json::Value::Null),
        rollback_window_secs: r.get("rollback_window_secs"),
        cutover_at: r.get("cutover_at"),
        state: r.get("state"),
        created_at: r.get("created_at"),
    }
}
