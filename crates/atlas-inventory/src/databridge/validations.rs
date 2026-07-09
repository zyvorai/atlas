// Copyright (c) 2026 ZyvorAI Labs Private Limited. All rights reserved.
//! Validation run records (source vs edge row-count / checksum / schema-diff comparisons).

use anyhow::Result;
use atlas_api_types::ValidationRun;
use sqlx::{Row, SqlitePool};

pub async fn insert_validation(
    pool: &SqlitePool,
    id: &str,
    tenant_id: &str,
    plan_id: &str,
    kind: &str,
) -> Result<()> {
    sqlx::query(
        "INSERT INTO validation_runs (id, tenant_id, plan_id, kind, state)
         VALUES (?, ?, ?, ?, 'running')",
    )
    .bind(id)
    .bind(tenant_id)
    .bind(plan_id)
    .bind(kind)
    .execute(pool)
    .await?;
    Ok(())
}

/// Record the outcome and flip to passed/failed.
pub async fn set_result(
    pool: &SqlitePool,
    id: &str,
    passed: bool,
    tables_total: i64,
    tables_mismatched: i64,
    summary: &serde_json::Value,
) -> Result<()> {
    let state = if passed { "passed" } else { "failed" };
    sqlx::query(
        "UPDATE validation_runs SET state=?, tables_total=?, tables_mismatched=?, summary=?,
         completed_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE id=?",
    )
    .bind(state)
    .bind(tables_total)
    .bind(tables_mismatched)
    .bind(summary.to_string())
    .bind(id)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn get_validation(pool: &SqlitePool, id: &str) -> Result<Option<ValidationRun>> {
    let row = sqlx::query(&select("WHERE id = ?"))
        .bind(id)
        .fetch_optional(pool)
        .await?;
    Ok(row.map(row_to_validation))
}

pub async fn list_validations(pool: &SqlitePool, plan_id: Option<&str>) -> Result<Vec<ValidationRun>> {
    let rows = match plan_id {
        Some(pid) => {
            sqlx::query(&select("WHERE plan_id = ? ORDER BY created_at DESC"))
                .bind(pid)
                .fetch_all(pool)
                .await?
        }
        None => {
            sqlx::query(&select("ORDER BY created_at DESC"))
                .fetch_all(pool)
                .await?
        }
    };
    Ok(rows.into_iter().map(row_to_validation).collect())
}

/// Validations in a given state — used by the reconciler to watch running validation Jobs.
pub async fn list_by_state(pool: &SqlitePool, state: &str) -> Result<Vec<ValidationRun>> {
    let rows = sqlx::query(&select("WHERE state = ? ORDER BY created_at DESC"))
        .bind(state)
        .fetch_all(pool)
        .await?;
    Ok(rows.into_iter().map(row_to_validation).collect())
}

/// The most recent validation for a plan — used by the cutover guard.
pub async fn latest_for_plan(pool: &SqlitePool, plan_id: &str) -> Result<Option<ValidationRun>> {
    let row = sqlx::query(&select("WHERE plan_id = ? ORDER BY created_at DESC LIMIT 1"))
        .bind(plan_id)
        .fetch_optional(pool)
        .await?;
    Ok(row.map(row_to_validation))
}

fn select(tail: &str) -> String {
    format!(
        "SELECT id, tenant_id, plan_id, kind, state, tables_total, tables_mismatched, summary,
                created_at, completed_at
         FROM validation_runs {tail}"
    )
}

fn row_to_validation(r: sqlx::sqlite::SqliteRow) -> ValidationRun {
    let summary: String = r.get("summary");
    ValidationRun {
        id: r.get("id"),
        tenant_id: r.get("tenant_id"),
        plan_id: r.get("plan_id"),
        kind: r.get("kind"),
        state: r.get("state"),
        tables_total: r.get("tables_total"),
        tables_mismatched: r.get("tables_mismatched"),
        summary: serde_json::from_str(&summary).unwrap_or(serde_json::Value::Null),
        created_at: r.get("created_at"),
        completed_at: r.get("completed_at"),
    }
}
