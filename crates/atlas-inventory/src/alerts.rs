// Copyright (c) 2026 ZyvorAI Labs Private Limited. All rights reserved.
//! Alert records (PDF §15.2). Alerts are keyed by a deterministic id (rule + resource) so the
//! monitor upserts rather than duplicating on each tick.

use anyhow::Result;
use atlas_api_types::AlertRecord;
use sqlx::{Row, SqlitePool};

/// Insert or re-open an alert (deterministic id per rule+resource).
#[allow(clippy::too_many_arguments)]
pub async fn upsert_open(
    pool: &SqlitePool,
    id: &str,
    severity: &str,
    source: &str,
    resource_type: &str,
    resource_id: &str,
    title: &str,
    description: &str,
    evidence: &serde_json::Value,
) -> Result<()> {
    sqlx::query(
        "INSERT INTO storage_alerts (id, severity, source, resource_type, resource_id, title, description, evidence, state)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, 'open')
         ON CONFLICT(id) DO UPDATE SET
            severity=excluded.severity, title=excluded.title, description=excluded.description,
            evidence=excluded.evidence, state='open', resolved_at=NULL",
    )
    .bind(id)
    .bind(severity)
    .bind(source)
    .bind(resource_type)
    .bind(resource_id)
    .bind(title)
    .bind(description)
    .bind(evidence.to_string())
    .execute(pool)
    .await?;
    Ok(())
}

/// Resolve an alert if it exists and is currently open.
pub async fn resolve(pool: &SqlitePool, id: &str) -> Result<()> {
    sqlx::query(
        "UPDATE storage_alerts SET state='resolved', resolved_at=strftime('%Y-%m-%dT%H:%M:%fZ','now')
         WHERE id=? AND state='open'",
    )
    .bind(id)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn list(pool: &SqlitePool, state: Option<&str>) -> Result<Vec<AlertRecord>> {
    let rows = match state {
        Some(st) => {
            sqlx::query(&select("WHERE state = ? ORDER BY created_at DESC"))
                .bind(st)
                .fetch_all(pool)
                .await?
        }
        None => {
            sqlx::query(&select("ORDER BY created_at DESC"))
                .fetch_all(pool)
                .await?
        }
    };
    Ok(rows.into_iter().map(row_to_alert).collect())
}

pub async fn count_open(pool: &SqlitePool) -> Result<i64> {
    Ok(
        sqlx::query_scalar("SELECT COUNT(*) FROM storage_alerts WHERE state='open'")
            .fetch_one(pool)
            .await?,
    )
}

fn select(tail: &str) -> String {
    format!(
        "SELECT id, severity, source, resource_type, resource_id, title, description, evidence, state, created_at, resolved_at
         FROM storage_alerts {tail}"
    )
}

fn row_to_alert(r: sqlx::sqlite::SqliteRow) -> AlertRecord {
    let evidence: serde_json::Value = serde_json::from_str(r.get::<String, _>("evidence").as_str())
        .unwrap_or(serde_json::Value::Null);
    AlertRecord {
        id: r.get("id"),
        severity: r.get("severity"),
        source: r.get("source"),
        resource_type: r.get("resource_type"),
        resource_id: r.get("resource_id"),
        title: r.get("title"),
        description: r.get("description"),
        evidence,
        state: r.get("state"),
        created_at: r.get("created_at"),
        resolved_at: r.get("resolved_at"),
    }
}
