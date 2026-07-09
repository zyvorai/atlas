// Copyright (c) 2026 ZyvorAI Labs Private Limited. All rights reserved.
//! The DataBridge reconciler: a periodic worker (mirrors `atlas_jobs::spawn_scheduler`) that
//! advances long-running pipeline work the single-shot job engine can't hold open — polling edge
//! operator CR status to `ready`, and (later slices) watching full-load/validation Jobs and CDC lag.
//!
//! In `fake` mode most stages complete synchronously in the job handlers, so the reconciler is
//! mostly idle; the real (k8s) paths give it its work. Errors are logged and swallowed so a
//! transient failure just retries next tick.

use std::sync::Arc;
use std::time::Duration;

use sqlx::SqlitePool;

use crate::cr::{cnpg, mysql_operator};

/// Spawn the reconciler loop. `interval_secs == 0` disables it (the test/Default config).
pub fn spawn_reconciler(
    pool: SqlitePool,
    k8s: Option<Arc<atlas_driver_k8s::K8sDriver>>,
    interval_secs: u64,
) {
    if interval_secs == 0 {
        tracing::info!("databridge reconciler disabled (interval = 0)");
        return;
    }
    tokio::spawn(async move {
        tracing::info!(interval_secs, "databridge reconciler started");
        let mut tick = tokio::time::interval(Duration::from_secs(interval_secs));
        loop {
            tick.tick().await;
            if let Err(e) = reconcile_once(&pool, k8s.as_deref()).await {
                tracing::warn!("databridge reconcile tick failed: {e:#}");
            }
        }
    });
}

/// One reconcile pass. Advances `provisioning` edge clusters to `ready` by polling their operator
/// CR status. (Later slices add: full-load / validation Job watching, CDC lag, cutover draining.)
async fn reconcile_once(
    pool: &SqlitePool,
    k8s: Option<&atlas_driver_k8s::K8sDriver>,
) -> anyhow::Result<()> {
    // Fake CDC: drain each streaming stream's lag toward zero so the Replication view animates and
    // cutover eventually becomes allowed. (Real lag comes from Kafka Connect / source LSN.)
    drain_fake_cdc(pool).await?;

    let Some(k8s) = k8s else { return Ok(()) }; // fake mode completes provisioning inline

    for edge in atlas_inventory::databridge::edge_clusters::list_by_state(pool, "provisioning").await? {
        let Some(cr_name) = edge.cr_name.as_deref() else { continue };
        let (group, version, kind, ready, endpoint, secret_ref) = match edge.engine.as_str() {
            "postgres" => (
                cnpg::GROUP, cnpg::VERSION, cnpg::KIND,
                cnpg::is_ready as fn(&serde_json::Value) -> bool,
                cnpg::endpoint(cr_name, &edge.namespace),
                cnpg::secret_ref(cr_name),
            ),
            "mysql" => (
                mysql_operator::GROUP, mysql_operator::VERSION, mysql_operator::KIND,
                mysql_operator::is_ready as fn(&serde_json::Value) -> bool,
                mysql_operator::endpoint(cr_name, &edge.namespace),
                mysql_operator::secret_ref(cr_name),
            ),
            other => {
                tracing::warn!("edge cluster {} has unknown engine {other}", edge.id);
                continue;
            }
        };

        match k8s
            .get_cr_status(group, version, kind, &edge.namespace, cr_name)
            .await
        {
            Ok(Some(status)) if ready(&status) => {
                atlas_inventory::databridge::edge_clusters::set_ready(
                    pool, &edge.id, &endpoint, &secret_ref,
                )
                .await?;
                if let Some(plan_id) = edge.plan_id.as_deref() {
                    atlas_inventory::databridge::plans::set_state(pool, plan_id, "provisioned")
                        .await?;
                }
                tracing::info!("edge cluster {} is ready at {endpoint}", edge.id);
            }
            Ok(_) => tracing::debug!("edge cluster {} still provisioning", edge.id),
            Err(e) => tracing::warn!("poll {kind} status for {}: {e}", edge.id),
        }
    }

    // Watch full-load batch Jobs -> advance full_loading -> loaded | failed.
    for plan in atlas_inventory::databridge::plans::list_by_state(pool, "full_loading").await? {
        let job = crate::loader::job_name(&plan.id);
        match k8s
            .get_cr_status(
                crate::loader::JOB_GROUP,
                crate::loader::JOB_VERSION,
                crate::loader::JOB_KIND,
                crate::pipeline::EDGE_NAMESPACE,
                &job,
            )
            .await
        {
            Ok(Some(status)) => match crate::loader::job_outcome(&status) {
                crate::loader::JobOutcome::Succeeded => {
                    atlas_inventory::databridge::plans::set_state(pool, &plan.id, "loaded").await?;
                    tracing::info!("plan {} full-load complete", plan.id);
                }
                crate::loader::JobOutcome::Failed => {
                    atlas_inventory::databridge::plans::set_state(pool, &plan.id, "failed").await?;
                    tracing::warn!("plan {} full-load Job {job} failed", plan.id);
                }
                crate::loader::JobOutcome::Running => {}
            },
            Ok(None) => tracing::debug!("plan {} full-load Job {job} has no status yet", plan.id),
            Err(e) => tracing::warn!("poll full-load Job {job}: {e}"),
        }
    }

    // Watch validation Jobs -> record passed/failed and advance the plan.
    for v in atlas_inventory::databridge::validations::list_by_state(pool, "running").await? {
        let job = crate::validate::job_name(&v.id);
        match k8s
            .get_cr_status(
                crate::loader::JOB_GROUP,
                crate::loader::JOB_VERSION,
                crate::loader::JOB_KIND,
                crate::pipeline::EDGE_NAMESPACE,
                &job,
            )
            .await
        {
            Ok(Some(status)) => match crate::loader::job_outcome(&status) {
                crate::loader::JobOutcome::Succeeded => {
                    let summary = serde_json::json!({ "note": "row counts matched (see Job logs)" });
                    atlas_inventory::databridge::validations::set_result(pool, &v.id, true, 0, 0, &summary).await?;
                    atlas_inventory::databridge::plans::set_state(pool, &v.plan_id, "validated").await?;
                    tracing::info!("validation {} passed", v.id);
                }
                crate::loader::JobOutcome::Failed => {
                    let summary = serde_json::json!({ "note": "row-count mismatch (see Job logs)" });
                    atlas_inventory::databridge::validations::set_result(pool, &v.id, false, 0, 1, &summary).await?;
                    atlas_inventory::databridge::plans::set_state(pool, &v.plan_id, "failed").await?;
                    tracing::warn!("validation {} failed", v.id);
                }
                crate::loader::JobOutcome::Running => {}
            },
            Ok(None) => tracing::debug!("validation {} Job {job} has no status yet", v.id),
            Err(e) => tracing::warn!("poll validation Job {job}: {e}"),
        }
    }
    Ok(())
}

/// Drain fake CDC streams toward zero lag each tick (demoable replication without a real Kafka).
async fn drain_fake_cdc(pool: &SqlitePool) -> anyhow::Result<()> {
    for stream in atlas_inventory::databridge::cdc::list_by_state(pool, "streaming").await? {
        if stream.lag_seconds <= 0 && stream.lag_bytes <= 0 {
            continue;
        }
        // ~40% of remaining backlog per tick, plus a few thousand more events applied.
        let lag_bytes = (stream.lag_bytes * 6 / 10).max(0);
        let lag_seconds = (stream.lag_seconds * 6 / 10).max(0);
        let events = stream.events_total + 5000;
        atlas_inventory::databridge::cdc::update_lag(
            pool,
            &stream.id,
            lag_bytes,
            lag_seconds,
            stream.last_source_lsn.as_deref(),
            stream.last_source_lsn.as_deref(), // caught up to source
            events,
        )
        .await?;
    }
    Ok(())
}
