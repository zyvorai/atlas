// Copyright (c) 2026 ZyvorAI Labs Private Limited. All rights reserved.
//! Discovery worker: run a driver's discovery pass and normalize it into the SQLite inventory
//! (PDF §8.1). Emits a `storage.backend.discovered` log event on success.

use std::sync::Arc;

use anyhow::{Context, Result};
use atlas_driver_core::StorageDriver;
use sqlx::SqlitePool;

/// Summary returned to the caller after a discovery pass.
#[derive(Debug, Clone, serde::Serialize)]
pub struct DiscoverySummary {
    pub backend_id: String,
    pub cluster_id: String,
    pub pools: usize,
    pub osds: usize,
    pub volumes: usize,
}

/// Run discovery for one driver and persist the result.
pub async fn run_discovery(
    pool: &SqlitePool,
    driver: Arc<dyn StorageDriver>,
) -> Result<DiscoverySummary> {
    let backend_id = driver.backend_id().to_string();
    let discovery = driver
        .discover()
        .await
        .with_context(|| format!("discovery failed for backend {backend_id}"))?;

    atlas_inventory::upsert_discovery(pool, &backend_id, &discovery)
        .await
        .context("persisting discovery result")?;

    let summary = DiscoverySummary {
        backend_id: backend_id.clone(),
        cluster_id: discovery.cluster.id.clone(),
        pools: discovery.pools.len(),
        osds: discovery.osds.len(),
        volumes: discovery.volumes.len(),
    };
    tracing::info!(
        backend = %summary.backend_id,
        cluster = %summary.cluster_id,
        pools = summary.pools,
        osds = summary.osds,
        volumes = summary.volumes,
        "storage.backend.discovered"
    );
    Ok(summary)
}
