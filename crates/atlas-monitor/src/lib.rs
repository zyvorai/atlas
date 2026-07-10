// Copyright (c) 2026 ZyvorAI Labs Private Limited. All rights reserved.
//! Monitor worker (PDF §15): periodically refresh inventory and evaluate alert rules
//! (cluster unhealthy, pool near-full, OSD down) into `storage_alerts`.
//!
//! Rules are deterministic and idempotent — each (rule, resource) maps to a stable alert id, so a
//! recurring condition updates one row instead of piling up duplicates, and a cleared condition
//! resolves it.

use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use atlas_api_types::Health;
use atlas_driver_core::StorageDriver;
use serde_json::json;
use sqlx::{Row, SqlitePool};

pub mod notify;
pub mod prometheus;

/// Pool utilization thresholds (PDF §15.2).
const POOL_WARN: f64 = 0.75;
const POOL_CRITICAL: f64 = 0.85;

/// Spawn the monitor loop. `interval_secs == 0` disables it. `prometheus_url`, when set, is scraped
/// each tick for Ceph capacity/latency metrics.
#[allow(clippy::too_many_arguments)]
pub fn spawn(
    pool: SqlitePool,
    driver: Arc<dyn StorageDriver>,
    interval_secs: u64,
    prometheus_url: Option<String>,
    webhook_url: Option<String>,
    is_leader: Arc<std::sync::atomic::AtomicBool>,
) {
    if interval_secs == 0 {
        tracing::info!("monitor disabled (interval = 0)");
        return;
    }
    tokio::spawn(async move {
        tracing::info!(interval_secs, "monitor worker started");
        let mut tick = tokio::time::interval(Duration::from_secs(interval_secs));
        loop {
            tick.tick().await;
            // HA: only the leader replica evaluates alerts / runs discovery, so a multi-replica
            // deployment doesn't duplicate work.
            if !is_leader.load(std::sync::atomic::Ordering::Relaxed) {
                continue;
            }
            if let Err(e) = atlas_discovery::run_discovery(&pool, driver.clone(), None).await {
                tracing::warn!("monitor discovery failed: {e:#}");
            }
            if let Err(e) = evaluate(&pool).await {
                tracing::warn!("monitor evaluate failed: {e:#}");
            }
            if let Some(url) = &prometheus_url {
                match prometheus::scrape(&pool, url).await {
                    Ok(n) => tracing::debug!("scraped {n} ceph metrics"),
                    Err(e) => tracing::warn!("prometheus scrape failed: {e:#}"),
                }
            }
            // Push newly-fired alerts to the webhook (after evaluate + scrape so it sees this
            // tick's alerts). Evaluate runs again next tick, so a failed post retries then.
            if let Some(url) = &webhook_url {
                match notify::dispatch(&pool, url).await {
                    Ok(n) if n > 0 => tracing::info!("pushed {n} alert notification(s)"),
                    Ok(_) => {}
                    Err(e) => tracing::warn!("alert notify failed: {e:#}"),
                }
            }
        }
    });
}

/// Days-until-full thresholds for the capacity-forecast rule.
const FORECAST_WARN_DAYS: f64 = 14.0;
const FORECAST_CRITICAL_DAYS: f64 = 3.0;

/// Evaluate all alert rules once against current inventory. Public for tests.
/// Tenant byte-quota usage thresholds (warn/critical), so tenants get a heads-up before the hard wall.
const QUOTA_WARN: f64 = 0.80;
const QUOTA_CRITICAL: f64 = 0.95;

pub async fn evaluate(pool: &SqlitePool) -> Result<()> {
    evaluate_clusters(pool).await?;
    evaluate_pools(pool).await?;
    evaluate_osds(pool).await?;
    evaluate_capacity_forecast(pool).await?;
    // Day-2: conditions that previously failed silently.
    evaluate_failed_jobs(pool).await?;
    evaluate_cdc_streams(pool).await?;
    evaluate_tenant_quotas(pool).await?;
    Ok(())
}

/// Raise a rollup alert when jobs have failed in the recent window (they otherwise fail silently —
/// the job engine has no alerting). Clears once the window passes with no new failures.
async fn evaluate_failed_jobs(pool: &SqlitePool) -> Result<()> {
    let id = "alert_jobs_failing";
    let n: i64 = sqlx::query(
        "SELECT COUNT(*) AS n FROM storage_jobs WHERE state='failed' \
         AND completed_at >= strftime('%Y-%m-%dT%H:%M:%fZ','now','-15 minutes')",
    )
    .fetch_one(pool)
    .await?
    .get("n");
    if n > 0 {
        atlas_inventory::alerts::upsert_open(
            pool,
            id,
            "warning",
            "monitor",
            "jobs",
            "recent",
            "Jobs failing",
            &format!("{n} job(s) failed in the last 15 minutes"),
            &json!({ "failed_15m": n }),
        )
        .await?;
    } else {
        atlas_inventory::alerts::resolve(pool, id).await?;
    }
    Ok(())
}

/// Alert on CDC streams whose connector is in `error` (replication stalled/broken). The reconciler
/// flags the state; without this rule nobody is notified that a live migration has stopped.
async fn evaluate_cdc_streams(pool: &SqlitePool) -> Result<()> {
    let rows = sqlx::query("SELECT id, state FROM cdc_streams")
        .fetch_all(pool)
        .await?;
    for r in rows {
        let sid: String = r.get("id");
        let state: String = r.get("state");
        let id = format!("alert_cdc_error_{sid}");
        if state == "error" {
            atlas_inventory::alerts::upsert_open(
                pool,
                &id,
                "critical",
                "monitor",
                "cdc_stream",
                &sid,
                "CDC replication error",
                &format!("CDC stream {sid} connector is not healthy — replication has stalled"),
                &json!({ "state": state }),
            )
            .await?;
        } else {
            atlas_inventory::alerts::resolve(pool, &id).await?;
        }
    }
    Ok(())
}

/// Warn a tenant approaching its byte quota (before `check_admission` hard-rejects at the wall).
async fn evaluate_tenant_quotas(pool: &SqlitePool) -> Result<()> {
    for q in atlas_inventory::tenants::list_overview(pool).await? {
        let id = format!("alert_tenant_quota_{}", q.tenant_id);
        let ratio = if q.max_bytes > 0 {
            q.used_bytes as f64 / q.max_bytes as f64
        } else {
            0.0
        };
        if q.max_bytes > 0 && ratio >= QUOTA_WARN {
            let severity = if ratio >= QUOTA_CRITICAL {
                "critical"
            } else {
                "warning"
            };
            atlas_inventory::alerts::upsert_open(
                pool,
                &id,
                severity,
                "monitor",
                "tenant",
                &q.tenant_id,
                "Tenant quota nearly exhausted",
                &format!(
                    "Tenant {} is at {:.0}% of its {}-byte volume quota",
                    q.tenant_id,
                    ratio * 100.0,
                    q.max_bytes
                ),
                &json!({ "used_bytes": q.used_bytes, "max_bytes": q.max_bytes, "ratio": ratio }),
            )
            .await?;
        } else {
            atlas_inventory::alerts::resolve(pool, &id).await?;
        }
    }
    Ok(())
}

/// Raise an alert when the least-squares fill projection (over the last 6h) crosses a threshold.
async fn evaluate_capacity_forecast(pool: &SqlitePool) -> Result<()> {
    let id = "alert_capacity_forecast";
    let f = atlas_inventory::metrics::forecast(pool, 360).await?;
    let days = f.get("days_to_full").and_then(|v| v.as_f64());
    match days {
        Some(d) if d <= FORECAST_WARN_DAYS => {
            let severity = if d <= FORECAST_CRITICAL_DAYS {
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
                "capacity",
                "Capacity filling up",
                &format!("Projected full in ~{d:.1} days at the current growth rate"),
                &f,
            )
            .await?;
        }
        // No projection, or plenty of runway → clear any existing alert.
        _ => atlas_inventory::alerts::resolve(pool, id).await?,
    }
    Ok(())
}

async fn evaluate_clusters(pool: &SqlitePool) -> Result<()> {
    for c in atlas_inventory::list_clusters(pool).await? {
        let id = format!("alert_cluster_unhealthy_{}", c.id);
        match c.health {
            Health::Warn => {
                atlas_inventory::alerts::upsert_open(
                    pool,
                    &id,
                    "warning",
                    "monitor",
                    "cluster",
                    &c.id,
                    "Cluster health degraded",
                    &format!("Cluster {} is HEALTH_WARN", c.name),
                    &json!({ "health": "warn" }),
                )
                .await?;
            }
            Health::Critical => {
                atlas_inventory::alerts::upsert_open(
                    pool,
                    &id,
                    "critical",
                    "monitor",
                    "cluster",
                    &c.id,
                    "Cluster unhealthy",
                    &format!("Cluster {} is HEALTH_ERR", c.name),
                    &json!({ "health": "critical" }),
                )
                .await?;
            }
            // Ok / Unknown → clear any existing alert.
            _ => atlas_inventory::alerts::resolve(pool, &id).await?,
        }
    }
    Ok(())
}

async fn evaluate_pools(pool: &SqlitePool) -> Result<()> {
    for p in atlas_inventory::list_pools(pool).await? {
        let id = format!("alert_pool_near_full_{}", p.id);
        match (p.used_bytes, p.max_bytes) {
            (Some(used), Some(max)) if max > 0 => {
                let ratio = used as f64 / max as f64;
                if ratio >= POOL_WARN {
                    let severity = if ratio >= POOL_CRITICAL {
                        "critical"
                    } else {
                        "warning"
                    };
                    atlas_inventory::alerts::upsert_open(
                        pool,
                        &id,
                        severity,
                        "monitor",
                        "pool",
                        &p.id,
                        "Pool near full",
                        &format!("Pool {} is {:.0}% full", p.name, ratio * 100.0),
                        &json!({ "used_bytes": used, "max_bytes": max, "ratio": ratio }),
                    )
                    .await?;
                } else {
                    atlas_inventory::alerts::resolve(pool, &id).await?;
                }
            }
            _ => atlas_inventory::alerts::resolve(pool, &id).await?,
        }
    }
    Ok(())
}

async fn evaluate_osds(pool: &SqlitePool) -> Result<()> {
    for o in atlas_inventory::list_osds(pool).await? {
        let id = format!("alert_osd_down_{}_{}", o.cluster_id, o.id);
        if !o.up || !o.in_cluster {
            atlas_inventory::alerts::upsert_open(
                pool,
                &id,
                "critical",
                "monitor",
                "osd",
                &format!("osd.{}", o.id),
                "OSD down",
                &format!("osd.{} is {}", o.id, if !o.up { "down" } else { "out" }),
                &json!({ "up": o.up, "in": o.in_cluster }),
            )
            .await?;
        } else {
            atlas_inventory::alerts::resolve(pool, &id).await?;
        }
    }
    Ok(())
}
