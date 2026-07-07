// Copyright (c) 2026 ZyvorAI Labs Private Limited. All rights reserved.
//! Startup wiring shared by the binary and integration tests: open the DB, register the Ceph
//! backend + driver, optionally attach a live Kubernetes driver, and optionally run an initial
//! discovery pass.

use std::sync::Arc;

use anyhow::Result;
use atlas_api_types::{BackendMode, BackendType, Capabilities, StorageBackend};
use atlas_common::config::CephDriverMode;
use atlas_common::Config;
use atlas_driver_ceph::{FakeCephDriver, RealCephDriver};
use atlas_driver_core::{DriverRegistry, StorageDriver};
use atlas_driver_k8s::K8sDriver;

use crate::state::AppState;

/// The single Ceph backend id used by the MVP.
pub const CEPH_BACKEND_ID: &str = "bkd_ceph_lab";

pub struct BuildOptions {
    /// Attempt to attach a live Kubernetes driver (disable in unit/integration tests).
    pub enable_k8s: bool,
    /// Run one discovery pass at startup so inventory is populated immediately.
    pub initial_discovery: bool,
}

impl Default for BuildOptions {
    fn default() -> Self {
        Self {
            enable_k8s: true,
            initial_discovery: true,
        }
    }
}

/// Build fully-wired application state from config.
pub async fn build_state(config: Config, opts: BuildOptions) -> Result<AppState> {
    let pool = atlas_inventory::connect(&config.database_url).await?;
    atlas_inventory::migrate(&pool).await?;

    // Register the Ceph backend row + driver.
    let (driver, mode): (Arc<dyn StorageDriver>, BackendMode) = match config.ceph_driver_mode {
        CephDriverMode::Real => (
            Arc::new(RealCephDriver::new(CEPH_BACKEND_ID)),
            BackendMode::External,
        ),
        CephDriverMode::Fake => (
            Arc::new(FakeCephDriver::new(CEPH_BACKEND_ID)),
            BackendMode::ManagedRook,
        ),
    };
    let backend = StorageBackend {
        id: CEPH_BACKEND_ID.into(),
        name: "zyvor-ceph-lab".into(),
        backend_type: BackendType::Ceph,
        mode,
        status: "active".into(),
        capabilities: Capabilities {
            block: true,
            file: true,
            object: true,
            snapshots: true,
            clone: true,
            expansion: true,
            replication: true,
        },
        connection_ref: None,
    };
    atlas_inventory::upsert_backend(&pool, &backend).await?;

    let mut registry = DriverRegistry::new();
    registry.register(driver.clone());

    // Attach a live Kubernetes driver if reachable.
    let k8s = if opts.enable_k8s {
        match K8sDriver::try_default().await {
            Ok(d) => {
                tracing::info!("kubernetes driver attached");
                Some(Arc::new(d))
            }
            Err(e) => {
                tracing::warn!("kubernetes driver unavailable: {e}; /storage-classes will 502");
                None
            }
        }
    } else {
        None
    };

    // Start the async job engine (write path) over the same pool + k8s driver.
    let jobs = atlas_jobs::JobEngine::start(pool.clone(), k8s.clone());

    let state = AppState {
        pool,
        config: Arc::new(config),
        drivers: Arc::new(registry),
        k8s,
        jobs,
    };

    if opts.initial_discovery {
        match atlas_discovery::run_discovery(&state.pool, driver).await {
            Ok(sum) => tracing::info!(?sum, "initial discovery complete"),
            Err(e) => tracing::warn!("initial discovery failed: {e:#}"),
        }
    }

    Ok(state)
}
