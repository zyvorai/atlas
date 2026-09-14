// Copyright (c) 2026 ZyvorAI Labs Private Limited.
// SPDX-License-Identifier: AGPL-3.0-only OR LicenseRef-Atlas-Commercial
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

/// Append one time-series row derived from a `metrics_summary()` value plus live counters.
/// Cheap (one INSERT) — called on the sampler tick so trends persist across restarts/reloads.
pub async fn record_history(
    pool: &SqlitePool,
    summary: &serde_json::Value,
    jobs_running: i64,
    alerts_open: i64,
) -> Result<()> {
    let i = |k: &str| summary.get(k).and_then(|v| v.as_i64()).unwrap_or(0);
    let io = summary.get("client_io");
    let f = |k: &str| {
        io.and_then(|o| o.get(k))
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0)
    };
    sqlx::query(
        "INSERT INTO metrics_history
           (raw_capacity_bytes, used_capacity_bytes, volumes, snapshots,
            read_bytes, write_bytes, read_ops, write_ops, jobs_running, alerts_open)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(i("raw_capacity_bytes"))
    .bind(i("used_capacity_bytes"))
    .bind(i("volumes"))
    .bind(i("snapshots"))
    .bind(f("read_bytes_total"))
    .bind(f("write_bytes_total"))
    .bind(f("read_ops_total"))
    .bind(f("write_ops_total"))
    .bind(jobs_running)
    .bind(alerts_open)
    .execute(pool)
    .await?;
    Ok(())
}

/// Delete samples older than `keep_hours` (called after each insert to bound table growth).
pub async fn prune_history(pool: &SqlitePool, keep_hours: i64) -> Result<u64> {
    let r = sqlx::query(
        "DELETE FROM metrics_history WHERE ts < strftime('%Y-%m-%dT%H:%M:%fZ','now', ? || ' hours')",
    )
    .bind(format!("-{keep_hours}"))
    .execute(pool)
    .await?;
    Ok(r.rows_affected())
}

/// Time-series samples from the last `minutes`, oldest first (for the Overview trend charts).
pub async fn history(pool: &SqlitePool, minutes: i64) -> Result<Vec<serde_json::Value>> {
    let rows = sqlx::query(
        "SELECT ts, raw_capacity_bytes, used_capacity_bytes, volumes, snapshots,
                read_bytes, write_bytes, read_ops, write_ops, jobs_running, alerts_open
         FROM metrics_history
         WHERE ts >= strftime('%Y-%m-%dT%H:%M:%fZ','now', ? || ' minutes')
         ORDER BY ts ASC",
    )
    .bind(format!("-{minutes}"))
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| {
            serde_json::json!({
                "ts": r.get::<String, _>("ts"),
                "raw_capacity_bytes": r.get::<i64, _>("raw_capacity_bytes"),
                "used_capacity_bytes": r.get::<i64, _>("used_capacity_bytes"),
                "volumes": r.get::<i64, _>("volumes"),
                "snapshots": r.get::<i64, _>("snapshots"),
                "read_bytes": r.get::<f64, _>("read_bytes"),
                "write_bytes": r.get::<f64, _>("write_bytes"),
                "read_ops": r.get::<f64, _>("read_ops"),
                "write_ops": r.get::<f64, _>("write_ops"),
                "jobs_running": r.get::<i64, _>("jobs_running"),
                "alerts_open": r.get::<i64, _>("alerts_open"),
            })
        })
        .collect())
}

/// Least-squares projection of days-until-full from persisted `used_capacity_bytes` history.
/// Returns `days_to_full: null` when usage is flat/shrinking or there aren't ≥2 samples.
pub async fn forecast(pool: &SqlitePool, minutes: i64) -> Result<serde_json::Value> {
    let rows = sqlx::query(
        "SELECT CAST(strftime('%s', ts) AS INTEGER) AS t,
                used_capacity_bytes AS used, raw_capacity_bytes AS raw
         FROM metrics_history
         WHERE ts >= strftime('%Y-%m-%dT%H:%M:%fZ','now', ? || ' minutes')
         ORDER BY ts ASC",
    )
    .bind(format!("-{minutes}"))
    .fetch_all(pool)
    .await?;

    let n = rows.len();
    let used_now = rows.last().map(|r| r.get::<i64, _>("used")).unwrap_or(0);
    let raw = rows.last().map(|r| r.get::<i64, _>("raw")).unwrap_or(0);

    // Slope of used-bytes over time (bytes/second) via ordinary least squares.
    let slope_per_sec = if n >= 2 {
        let t0 = rows[0].get::<i64, _>("t") as f64;
        let xs: Vec<f64> = rows
            .iter()
            .map(|r| r.get::<i64, _>("t") as f64 - t0)
            .collect();
        let ys: Vec<f64> = rows
            .iter()
            .map(|r| r.get::<i64, _>("used") as f64)
            .collect();
        let nf = n as f64;
        let sx: f64 = xs.iter().sum();
        let sy: f64 = ys.iter().sum();
        let sxx: f64 = xs.iter().map(|x| x * x).sum();
        let sxy: f64 = xs.iter().zip(&ys).map(|(x, y)| x * y).sum();
        let denom = nf * sxx - sx * sx;
        if denom.abs() > f64::EPSILON {
            (nf * sxy - sx * sy) / denom
        } else {
            0.0
        }
    } else {
        0.0
    };

    let per_day = slope_per_sec * 86_400.0;
    let free = (raw - used_now).max(0) as f64;
    // Only meaningful when growing by more than ~1 MiB/day (below that it's noise → "not filling").
    let days_to_full = if per_day > 1_048_576.0 {
        Some(((free / per_day) * 10.0).round() / 10.0)
    } else {
        None
    };

    Ok(serde_json::json!({
        "samples": n,
        "window_minutes": minutes,
        "used_capacity_bytes": used_now,
        "raw_capacity_bytes": raw,
        "growth_bytes_per_day": per_day.round(),
        "days_to_full": days_to_full,
    }))
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

/// Sum the latest values across all label sets of a metric (0.0 if the metric is absent).
pub async fn sum_value(pool: &SqlitePool, name: &str) -> Result<f64> {
    let v: Option<f64> =
        sqlx::query_scalar("SELECT SUM(value) FROM storage_metrics WHERE name = ?")
            .bind(name)
            .fetch_one(pool)
            .await?;
    Ok(v.unwrap_or(0.0))
}
