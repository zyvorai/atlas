// Copyright (c) 2026 ZyvorAI Labs Private Limited.
// SPDX-License-Identifier: AGPL-3.0-only OR LicenseRef-Atlas-Commercial
//! CDC stream records (Debezium source->edge replication + live lag).

use anyhow::Result;
use atlas_api_types::CdcStream;
use sqlx::{Row, SqlitePool};

#[allow(clippy::too_many_arguments)]
pub async fn insert_stream(
    pool: &SqlitePool,
    id: &str,
    tenant_id: &str,
    plan_id: &str,
    engine: &str,
    connect_name: &str,
    connector_name: &str,
    topic_prefix: &str,
) -> Result<()> {
    sqlx::query(
        "INSERT INTO cdc_streams
         (id, tenant_id, plan_id, engine, connect_name, connector_name, topic_prefix, state)
         VALUES (?, ?, ?, ?, ?, ?, ?, 'starting')",
    )
    .bind(id)
    .bind(tenant_id)
    .bind(plan_id)
    .bind(engine)
    .bind(connect_name)
    .bind(connector_name)
    .bind(topic_prefix)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn set_state(pool: &SqlitePool, id: &str, state: &str) -> Result<()> {
    sqlx::query("UPDATE cdc_streams SET state=? WHERE id=?")
        .bind(state)
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

/// Increment a stream's restart counter (day-2 self-heal) and return the new count.
pub async fn bump_restart(pool: &SqlitePool, id: &str) -> Result<i64> {
    sqlx::query("UPDATE cdc_streams SET restart_count = restart_count + 1 WHERE id=?")
        .bind(id)
        .execute(pool)
        .await?;
    let n: i64 = sqlx::query_scalar("SELECT restart_count FROM cdc_streams WHERE id=?")
        .bind(id)
        .fetch_one(pool)
        .await?;
    Ok(n)
}

/// Update the live replication-lag fields (called by the reconciler each tick).
#[allow(clippy::too_many_arguments)]
pub async fn update_lag(
    pool: &SqlitePool,
    id: &str,
    lag_bytes: i64,
    lag_seconds: i64,
    last_source_lsn: Option<&str>,
    last_applied_lsn: Option<&str>,
    events_total: i64,
) -> Result<()> {
    sqlx::query(
        "UPDATE cdc_streams SET lag_bytes=?, lag_seconds=?, last_source_lsn=?, last_applied_lsn=?,
         events_total=?, lag_updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE id=?",
    )
    .bind(lag_bytes)
    .bind(lag_seconds)
    .bind(last_source_lsn)
    .bind(last_applied_lsn)
    .bind(events_total)
    .bind(id)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn get_stream(pool: &SqlitePool, id: &str) -> Result<Option<CdcStream>> {
    let row = sqlx::query(&select("WHERE id = ?"))
        .bind(id)
        .fetch_optional(pool)
        .await?;
    Ok(row.map(row_to_stream))
}

pub async fn list_streams(pool: &SqlitePool) -> Result<Vec<CdcStream>> {
    let rows = sqlx::query(&select("ORDER BY created_at DESC"))
        .fetch_all(pool)
        .await?;
    Ok(rows.into_iter().map(row_to_stream).collect())
}

pub async fn list_by_state(pool: &SqlitePool, state: &str) -> Result<Vec<CdcStream>> {
    let rows = sqlx::query(&select("WHERE state = ? ORDER BY created_at DESC"))
        .bind(state)
        .fetch_all(pool)
        .await?;
    Ok(rows.into_iter().map(row_to_stream).collect())
}

fn select(tail: &str) -> String {
    format!(
        "SELECT id, tenant_id, plan_id, engine, connect_name, connector_name, topic_prefix, state,
                lag_bytes, lag_seconds, last_source_lsn, last_applied_lsn, events_total,
                lag_updated_at, restart_count, created_at
         FROM cdc_streams {tail}"
    )
}

fn row_to_stream(r: sqlx::sqlite::SqliteRow) -> CdcStream {
    CdcStream {
        id: r.get("id"),
        tenant_id: r.get("tenant_id"),
        plan_id: r.get("plan_id"),
        engine: r.get("engine"),
        connect_name: r.get("connect_name"),
        connector_name: r.get("connector_name"),
        topic_prefix: r.get("topic_prefix"),
        state: r.get("state"),
        lag_bytes: r.get("lag_bytes"),
        lag_seconds: r.get("lag_seconds"),
        last_source_lsn: r.get("last_source_lsn"),
        last_applied_lsn: r.get("last_applied_lsn"),
        events_total: r.get("events_total"),
        lag_updated_at: r.get("lag_updated_at"),
        restart_count: r.get("restart_count"),
        created_at: r.get("created_at"),
    }
}
