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

/// Maps a Ceph **RBD image name** → `(namespace, pvc_name, storage_class)` of the owning PVC.
/// Built by the gateway from cluster PersistentVolumes (see `K8sDriver::rbd_image_owners`) and
/// passed in so `atlas-discovery` stays decoupled from the Kubernetes driver.
pub type RbdOwners = std::collections::HashMap<String, (String, String, Option<String>)>;

/// Run discovery for one driver and persist the result. When `rbd_owners` is provided, block
/// volumes that the driver couldn't attribute to Kubernetes are enriched with their owning PVC —
/// this is what lets raw `rbd ls` images show up as real VM disks (namespace/PVC) in inventory.
pub async fn run_discovery(
    pool: &SqlitePool,
    driver: Arc<dyn StorageDriver>,
    rbd_owners: Option<&RbdOwners>,
) -> Result<DiscoverySummary> {
    let backend_id = driver.backend_id().to_string();
    let mut discovery = driver
        .discover()
        .await
        .with_context(|| format!("discovery failed for backend {backend_id}"))?;

    if let Some(owners) = rbd_owners {
        let mut enriched = 0usize;
        for v in discovery.volumes.iter_mut() {
            if v.kubernetes_namespace.is_some() {
                continue;
            }
            if let Some((ns, pvc, sc)) = owners.get(&v.name) {
                v.kubernetes_namespace = Some(ns.clone());
                v.pvc_name = Some(pvc.clone());
                if v.storage_class_name.is_none() {
                    v.storage_class_name = sc.clone();
                }
                enriched += 1;
            }
        }
        if enriched > 0 {
            tracing::info!(backend = %backend_id, enriched, "discovery.rbd_pvc_correlated");
        }
    }

    atlas_inventory::upsert_discovery(pool, &backend_id, &discovery, !driver.is_fixture())
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
