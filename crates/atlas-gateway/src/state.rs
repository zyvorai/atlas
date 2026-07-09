// Copyright (c) 2026 ZyvorAI Labs Private Limited. All rights reserved.
//! Shared application state injected into every axum handler.

use std::sync::Arc;

use atlas_common::Config;
use atlas_driver_core::DriverRegistry;
use atlas_driver_k8s::K8sDriver;
use sqlx::SqlitePool;

#[derive(Clone)]
pub struct AppState {
    pub pool: SqlitePool,
    pub config: Arc<Config>,
    /// Backend storage drivers keyed by backend id (Ceph real/fake in the MVP).
    pub drivers: Arc<DriverRegistry>,
    /// Live Kubernetes driver, if a cluster was reachable at startup.
    pub k8s: Option<Arc<K8sDriver>>,
    /// Async job engine (write path).
    pub jobs: atlas_jobs::JobEngine,
}

impl AppState {
    /// Resolve the driver for a backend id, or the only one when there's a single backend.
    pub fn driver_for(
        &self,
        backend_id: &str,
    ) -> Option<Arc<dyn atlas_driver_core::StorageDriver>> {
        self.drivers.get(backend_id).or_else(|| self.drivers.any())
    }

    /// Build the RBD-image → owning-PVC map from cluster PersistentVolumes, so discovery can
    /// attribute raw `rbd ls` images to their Kubernetes VM disk. `None` when no cluster is
    /// attached or the PV list fails (discovery then just leaves those volumes unattributed).
    pub async fn rbd_owners(&self) -> Option<atlas_discovery::RbdOwners> {
        let k8s = self.k8s.as_ref()?;
        match k8s.rbd_image_owners().await {
            Ok(m) => Some(
                m.into_iter()
                    .map(|(img, o)| (img, (o.namespace, o.pvc_name, o.storage_class)))
                    .collect(),
            ),
            Err(e) => {
                tracing::warn!("rbd_image_owners failed: {e}");
                None
            }
        }
    }
}
