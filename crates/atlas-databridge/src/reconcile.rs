// Copyright (c) 2026 ZyvorAI Labs Private Limited. All rights reserved.
//! The DataBridge reconciler: a periodic worker (mirrors `atlas_jobs::spawn_scheduler`) that
//! advances long-running pipeline work the single-shot job engine can't hold open — polling edge
//! CR status to `ready`, watching full-load/validation Jobs, and tracking CDC replication lag.
//!
//! In `fake` mode most stages complete synchronously in the job handlers, so the reconciler is
//! mostly idle; the real (k8s) paths in later slices give it its work. Errors are logged and
//! swallowed so a transient failure just retries next tick.

use std::time::Duration;

use sqlx::SqlitePool;

/// Spawn the reconciler loop. `interval_secs == 0` disables it (the test/Default config).
pub fn spawn_reconciler(pool: SqlitePool, interval_secs: u64) {
    if interval_secs == 0 {
        tracing::info!("databridge reconciler disabled (interval = 0)");
        return;
    }
    tokio::spawn(async move {
        tracing::info!(interval_secs, "databridge reconciler started");
        let mut tick = tokio::time::interval(Duration::from_secs(interval_secs));
        loop {
            tick.tick().await;
            if let Err(e) = reconcile_once(&pool).await {
                tracing::warn!("databridge reconcile tick failed: {e:#}");
            }
        }
    });
}

/// One reconcile pass. Slice 5 skeleton: the fake pipeline completes each stage inline in its job
/// handler, so there is nothing outstanding to advance here yet. Later slices add:
///   - poll `edge_db_clusters` in `provisioning` → operator CR `status.ready` → `set_ready`
///   - watch full-load / validation batch Jobs → advance the plan
///   - read Kafka-Connect / source-LSN lag → `cdc::update_lag`
///   - drive cutover draining when lag falls under threshold
async fn reconcile_once(pool: &SqlitePool) -> anyhow::Result<()> {
    // Count outstanding provisioning clusters so the loop has an observable heartbeat.
    let provisioning =
        atlas_inventory::databridge::edge_clusters::list_by_state(pool, "provisioning").await?;
    if !provisioning.is_empty() {
        tracing::debug!(
            "databridge reconciler: {} cluster(s) provisioning (awaiting real CR status)",
            provisioning.len()
        );
    }
    Ok(())
}
