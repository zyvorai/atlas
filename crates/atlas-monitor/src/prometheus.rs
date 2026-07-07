// Copyright (c) 2026 ZyvorAI Labs Private Limited. All rights reserved.
//! Scrape the Ceph mgr Prometheus module (PDF §15.1, §10.3 "Atlas → Prometheus").
//!
//! We keep a curated whitelist of metrics (capacity, OSD latency/up/in, pool usage, health) — not
//! the full firehose — and store the latest value per (name, labels) for Atlas's summary views and
//! the high-latency alert rule.

use std::collections::BTreeMap;

use anyhow::{Context, Result};
use sqlx::SqlitePool;

/// Metric names we keep from the Ceph exporter.
const WHITELIST: &[&str] = &[
    "ceph_cluster_total_bytes",
    "ceph_cluster_total_used_bytes",
    "ceph_cluster_total_used_raw_bytes",
    "ceph_health_status",
    "ceph_osd_up",
    "ceph_osd_in",
    "ceph_osd_apply_latency_ms",
    "ceph_osd_commit_latency_ms",
    "ceph_pool_bytes_used",
    "ceph_pool_max_avail",
    "ceph_pg_total",
    "ceph_pg_active",
];

/// OSD apply-latency thresholds for the high-latency alert (ms).
const LATENCY_WARN_MS: f64 = 100.0;
const LATENCY_CRITICAL_MS: f64 = 1000.0;

/// Scrape the given `/metrics` URL, upsert whitelisted samples, and evaluate the latency alert.
/// Returns the number of samples stored.
pub async fn scrape(pool: &SqlitePool, url: &str) -> Result<usize> {
    let body = reqwest::Client::new()
        .get(url)
        .send()
        .await
        .with_context(|| format!("GET {url}"))?
        .error_for_status()?
        .text()
        .await?;

    let lines = body.lines().map(|l| Ok(l.to_string()));
    let scrape = prometheus_parse::Scrape::parse(lines).context("parse prometheus text")?;

    let mut stored = 0usize;
    for sample in &scrape.samples {
        if !WHITELIST.contains(&sample.metric.as_str()) {
            continue;
        }
        let value = match sample.value {
            prometheus_parse::Value::Counter(v)
            | prometheus_parse::Value::Gauge(v)
            | prometheus_parse::Value::Untyped(v) => v,
            _ => continue,
        };
        let labels: BTreeMap<String, String> = sample
            .labels
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        atlas_inventory::metrics::upsert(pool, &sample.metric, &labels, value).await?;
        stored += 1;
    }

    evaluate_latency_alert(pool).await?;
    Ok(stored)
}

/// Raise/clear a cluster-level high-latency alert from the worst OSD apply latency.
async fn evaluate_latency_alert(pool: &SqlitePool) -> Result<()> {
    let id = "alert_osd_latency_high";
    let worst = atlas_inventory::metrics::max_value(pool, "ceph_osd_apply_latency_ms").await?;
    match worst {
        Some(ms) if ms >= LATENCY_WARN_MS => {
            let severity = if ms >= LATENCY_CRITICAL_MS {
                "critical"
            } else {
                "warning"
            };
            atlas_inventory::alerts::upsert_open(
                pool,
                id,
                severity,
                "monitor",
                "cluster",
                "ceph",
                "High OSD latency",
                &format!("Worst OSD apply latency is {ms:.0} ms"),
                &serde_json::json!({ "apply_latency_ms": ms }),
            )
            .await?;
        }
        _ => atlas_inventory::alerts::resolve(pool, id).await?,
    }
    Ok(())
}
