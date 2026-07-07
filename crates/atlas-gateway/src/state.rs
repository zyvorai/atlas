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
}
