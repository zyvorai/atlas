// Copyright (c) 2026 ZyvorAI Labs Private Limited. All rights reserved.
//! Pipeline stage functions called by the job-engine `dispatch` arms. Each takes the SQLite pool
//! (+ later the k8s driver), does the work, updates the DataBridge inventory, and returns a JSON
//! result the job engine persists. Keeping the logic here keeps the `dispatch` match arms thin.

use anyhow::{anyhow, Result};
use sqlx::SqlitePool;

use crate::connector::DiscoveredSchema;
use crate::{assess, build_connector};

/// Discover a source's schema and persist it. Advances the source `registered → discovered`.
pub async fn discover(pool: &SqlitePool, source_id: &str) -> Result<serde_json::Value> {
    let source = atlas_inventory::databridge::sources::get_source(pool, source_id)
        .await?
        .ok_or_else(|| anyhow!("source {source_id} not found"))?;

    atlas_inventory::databridge::sources::set_state(pool, source_id, "discovering").await?;

    // On any discovery failure, flip the source to `error` so the UI shows why.
    let schema = match async {
        let connector = build_connector(&source)?;
        connector.discover().await
    }
    .await
    {
        Ok(s) => s,
        Err(e) => {
            let _ = atlas_inventory::databridge::sources::set_state(pool, source_id, "error").await;
            return Err(e);
        }
    };
    let discovered = serde_json::to_value(&schema)?;
    atlas_inventory::databridge::sources::set_discovered(pool, source_id, &discovered).await?;

    Ok(serde_json::json!({
        "source_id": source_id,
        "engine": schema.engine,
        "version": schema.version,
        "databases": schema.databases,
        "tables": schema.tables.len(),
        "total_size_bytes": schema.total_size_bytes,
        "cdc_capable": schema.cdc_capable,
    }))
}

/// Assess a plan's source: score readiness from the discovered schema. Advances the plan to
/// `assessed`. Requires the source to have been discovered.
pub async fn assess_plan(pool: &SqlitePool, plan_id: &str) -> Result<serde_json::Value> {
    let plan = atlas_inventory::databridge::plans::get_plan(pool, plan_id)
        .await?
        .ok_or_else(|| anyhow!("plan {plan_id} not found"))?;
    let source = atlas_inventory::databridge::sources::get_source(pool, &plan.source_id)
        .await?
        .ok_or_else(|| anyhow!("source {} not found", plan.source_id))?;
    if source.state != "discovered" {
        return Err(anyhow!(
            "source {} is not discovered yet (state: {})",
            source.id,
            source.state
        ));
    }
    let schema: DiscoveredSchema = serde_json::from_value(source.discovered.clone())
        .map_err(|e| anyhow!("source has no valid discovered schema: {e}"))?;

    let assessment = assess::assess(&schema);
    let value = serde_json::to_value(&assessment)?;
    atlas_inventory::databridge::plans::set_assessment(pool, plan_id, assessment.score, &value)
        .await?;

    Ok(serde_json::json!({
        "plan_id": plan_id,
        "score": assessment.score,
        "risk": assessment.risk,
        "blockers": assessment.blockers.len(),
        "warnings": assessment.warnings.len(),
    }))
}

/// Provision the edge database cluster for a plan. In `fake` source mode this fabricates a ready
/// cluster with a synthetic endpoint (no k8s needed); real mode (later slice) applies the
/// CloudNativePG / MySQL operator CR and lets the reconciler poll it to `ready`.
pub async fn provision_edge(pool: &SqlitePool, plan_id: &str) -> Result<serde_json::Value> {
    let plan = atlas_inventory::databridge::plans::get_plan(pool, plan_id)
        .await?
        .ok_or_else(|| anyhow!("plan {plan_id} not found"))?;
    let source = atlas_inventory::databridge::sources::get_source(pool, &plan.source_id)
        .await?
        .ok_or_else(|| anyhow!("source {} not found", plan.source_id))?;

    let engine = crate::SourceKind::parse(&source.kind)
        .ok_or_else(|| anyhow!("unsupported engine: {}", source.kind))?;
    let (engine_str, operator, port) = match engine {
        crate::SourceKind::Postgres => ("postgres", "cnpg", 5432),
        crate::SourceKind::Mysql => ("mysql", "percona", 3306),
    };
    let namespace = "zyvor-databridge";
    let cr_name = format!("edge-{}", &plan_id[plan_id.len().saturating_sub(8)..]);
    let edge_id = atlas_common::ids::edge_cluster_id();

    atlas_inventory::databridge::edge_clusters::insert_edge_cluster(
        pool, &edge_id, &plan.tenant_id, plan_id, engine_str, operator, namespace, &cr_name,
        "zyvor-rbd-prod", Some("zyvor-rbd-prod"), 1,
    )
    .await?;
    atlas_inventory::databridge::plans::set_edge_cluster(pool, plan_id, &edge_id).await?;
    atlas_inventory::databridge::plans::set_state(pool, plan_id, "provisioning").await?;

    if source.driver_mode == "fake" {
        // No operator/CR — fabricate a ready cluster so the pipeline runs end-to-end.
        let endpoint = format!("{cr_name}-rw.{namespace}.svc:{port}");
        let secret_ref = format!("{cr_name}-app");
        atlas_inventory::databridge::edge_clusters::set_ready(pool, &edge_id, &endpoint, &secret_ref)
            .await?;
        atlas_inventory::databridge::plans::set_state(pool, plan_id, "provisioned").await?;
        return Ok(serde_json::json!({
            "plan_id": plan_id, "edge_cluster_id": edge_id, "engine": engine_str,
            "operator": operator, "state": "ready", "service_endpoint": endpoint, "mode": "fake",
        }));
    }

    // Real mode: the CR-apply + reconciler polling lands in a later slice.
    Err(anyhow!(
        "real edge provisioning (CloudNativePG/MySQL operator) is not implemented in this slice"
    ))
}
