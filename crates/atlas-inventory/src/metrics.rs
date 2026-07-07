// Copyright (c) 2026 ZyvorAI Labs Private Limited. All rights reserved.
//! Latest scraped Ceph metrics (PDF §15.1). One row per (name, labels), upserted each scrape.

use std::collections::BTreeMap;

use anyhow::Result;
use atlas_api_types::MetricSample;
use sqlx::{Row, SqlitePool};

/// Upsert the latest value for a metric (name + label set).
pub async fn upsert(
    pool: &SqlitePool,
    name: &str,
    labels: &BTreeMap<String, String>,
    value: f64,
) -> Result<()> {
    let labels_json = serde_json::to_string(labels).unwrap_or_else(|_| "{}".into());
    sqlx::query(
        "INSERT INTO storage_metrics (name, labels, value, updated_at)
         VALUES (?, ?, ?, strftime('%Y-%m-%dT%H:%M:%fZ','now'))
         ON CONFLICT(name, labels) DO UPDATE SET value=excluded.value, updated_at=excluded.updated_at",
    )
    .bind(name)
    .bind(labels_json)
    .bind(value)
    .execute(pool)
    .await?;
    Ok(())
}

/// List stored metrics, optionally filtered by a name prefix.
pub async fn list(pool: &SqlitePool, name_prefix: Option<&str>) -> Result<Vec<MetricSample>> {
    let rows =
        match name_prefix {
            Some(p) => sqlx::query(
                "SELECT name, labels, value FROM storage_metrics WHERE name LIKE ? ORDER BY name",
            )
            .bind(format!("{p}%"))
            .fetch_all(pool)
            .await?,
            None => {
                sqlx::query("SELECT name, labels, value FROM storage_metrics ORDER BY name")
                    .fetch_all(pool)
                    .await?
            }
        };
    Ok(rows
        .into_iter()
        .map(|r| {
            let labels: BTreeMap<String, String> =
                serde_json::from_str(r.get::<String, _>("labels").as_str()).unwrap_or_default();
            MetricSample {
                name: r.get("name"),
                value: r.get("value"),
                labels,
            }
        })
        .collect())
}

/// The maximum value of a metric across all its label sets (e.g. worst OSD latency).
pub async fn max_value(pool: &SqlitePool, name: &str) -> Result<Option<f64>> {
    Ok(
        sqlx::query_scalar("SELECT MAX(value) FROM storage_metrics WHERE name = ?")
            .bind(name)
            .fetch_one(pool)
            .await?,
    )
}
