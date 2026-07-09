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
use atlas_driver_nfs::NfsDriver;
use atlas_driver_zfs::ZfsDriver;

use crate::state::AppState;

/// The single Ceph backend id used by the MVP.
pub const CEPH_BACKEND_ID: &str = "bkd_ceph_lab";
/// Optional second NFS backend id (enabled via `ATLAS_NFS_ENABLE`).
pub const NFS_BACKEND_ID: &str = "bkd_nfs_lab";
/// Optional third ZFS backend id (enabled via `ATLAS_ZFS_ENABLE`).
pub const ZFS_BACKEND_ID: &str = "bkd_zfs_lab";

pub struct BuildOptions {
    /// Attempt to attach a live Kubernetes driver (disable in unit/integration tests).
    pub enable_k8s: bool,
    /// Run one discovery pass at startup so inventory is populated immediately.
    pub initial_discovery: bool,
    /// Spawn the monitor/alerts worker (disable in tests).
    pub enable_monitor: bool,
}

impl Default for BuildOptions {
    fn default() -> Self {
        Self {
            enable_k8s: true,
            initial_discovery: true,
            enable_monitor: true,
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

    // Optionally register a second NFS backend — demonstrates that a non-Ceph driver flows through
    // the same discovery → inventory → REST/gRPC surface (PDF §17.2 pluggable drivers).
    let nfs_driver: Option<Arc<dyn StorageDriver>> = if config.nfs_enable {
        let server = config
            .nfs_server
            .clone()
            .unwrap_or_else(|| "nfs01.zyvor.lab".into());
        let exports = if config.nfs_exports.is_empty() {
            vec![
                "/exports/vmstore".to_string(),
                "/exports/backups".to_string(),
            ]
        } else {
            config.nfs_exports.clone()
        };
        let nfs: Arc<dyn StorageDriver> = Arc::new(NfsDriver::new(NFS_BACKEND_ID, server, exports));
        let nfs_backend = StorageBackend {
            id: NFS_BACKEND_ID.into(),
            name: "zyvor-nfs".into(),
            backend_type: BackendType::Nfs,
            mode: BackendMode::External,
            status: "active".into(),
            capabilities: Capabilities {
                block: false,
                file: true,
                object: false,
                snapshots: false,
                clone: false,
                expansion: false,
                replication: false,
            },
            connection_ref: None,
        };
        atlas_inventory::upsert_backend(&pool, &nfs_backend).await?;
        registry.register(nfs.clone());
        tracing::info!("nfs backend registered ({NFS_BACKEND_ID})");
        Some(nfs)
    } else {
        None
    };

    // Optionally register a third ZFS backend — same pluggable-driver contract as Ceph/NFS.
    let zfs_driver: Option<Arc<dyn StorageDriver>> = if config.zfs_enable {
        let host = config
            .zfs_host
            .clone()
            .unwrap_or_else(|| "zfs01.zyvor.lab".into());
        let zpools = if config.zfs_pools.is_empty() {
            vec!["tank".to_string(), "vault".to_string()]
        } else {
            config.zfs_pools.clone()
        };
        let zfs: Arc<dyn StorageDriver> = Arc::new(ZfsDriver::new(ZFS_BACKEND_ID, host, zpools));
        let zfs_backend = StorageBackend {
            id: ZFS_BACKEND_ID.into(),
            name: "zyvor-zfs".into(),
            backend_type: BackendType::Zfs,
            mode: BackendMode::External,
            status: "active".into(),
            capabilities: Capabilities {
                block: true,
                file: true,
                object: false,
                snapshots: true,
                clone: true,
                expansion: true,
                replication: false,
            },
            connection_ref: None,
        };
        atlas_inventory::upsert_backend(&pool, &zfs_backend).await?;
        registry.register(zfs.clone());
        tracing::info!("zfs backend registered ({ZFS_BACKEND_ID})");
        Some(zfs)
    } else {
        None
    };

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
        let rbd_owners = state.rbd_owners().await;
        match atlas_discovery::run_discovery(&state.pool, driver.clone(), rbd_owners.as_ref()).await
        {
            Ok(sum) => tracing::info!(?sum, "initial discovery complete"),
            Err(e) => tracing::warn!("initial discovery failed: {e:#}"),
        }
        if let Some(nfs) = &nfs_driver {
            match atlas_discovery::run_discovery(&state.pool, nfs.clone(), None).await {
                Ok(sum) => tracing::info!(?sum, "initial nfs discovery complete"),
                Err(e) => tracing::warn!("initial nfs discovery failed: {e:#}"),
            }
        }
        if let Some(zfs) = &zfs_driver {
            match atlas_discovery::run_discovery(&state.pool, zfs.clone(), None).await {
                Ok(sum) => tracing::info!(?sum, "initial zfs discovery complete"),
                Err(e) => tracing::warn!("initial zfs discovery failed: {e:#}"),
            }
        }
    }

    // Start the monitor/alerts worker (periodic discovery + alert-rule evaluation).
    if opts.enable_monitor {
        atlas_monitor::spawn(
            state.pool.clone(),
            driver,
            state.config.monitor_interval_secs,
            state.config.ceph_prometheus_url.clone(),
            state.config.alert_webhook_url.clone(),
        );
        // Protection-schedule worker: periodic snapshots + retention (shares the job engine).
        atlas_jobs::spawn_scheduler(
            state.pool.clone(),
            state.jobs.clone(),
            state.config.snapshot_tick_secs,
        );
        // Metrics-history sampler: append a capacity/IO/job time-series row each monitor tick,
        // pruning to a 48h window, so the Overview trend charts survive restarts + reloads.
        spawn_metrics_sampler(state.pool.clone(), state.config.monitor_interval_secs);
        // DataBridge reconciler: advances migration pipelines (edge CR status, full-load/validation
        // Jobs, CDC lag) that the single-shot job engine can't hold open.
        atlas_databridge::reconcile::spawn_reconciler(
            state.pool.clone(),
            state.k8s.clone(),
            state.config.databridge_reconcile_secs,
        );
    }

    Ok(state)
}

/// Periodically persist one `metrics_history` sample derived from the current summary + counters.
/// `interval_secs == 0` disables it (mirrors the monitor/scheduler workers).
fn spawn_metrics_sampler(pool: sqlx::SqlitePool, interval_secs: u64) {
    if interval_secs == 0 {
        return;
    }
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(std::time::Duration::from_secs(interval_secs));
        loop {
            tick.tick().await;
            if let Err(e) = sample_metrics_history(&pool).await {
                tracing::debug!("metrics-history sample skipped: {e:#}");
            }
        }
    });
}

async fn sample_metrics_history(pool: &sqlx::SqlitePool) -> Result<()> {
    let summary = atlas_inventory::metrics_summary(pool).await?;
    let running: i64 = atlas_inventory::jobs::count_by_state(pool)
        .await?
        .into_iter()
        .filter(|(s, _)| matches!(s.as_str(), "running" | "queued" | "verifying"))
        .map(|(_, n)| n)
        .sum();
    let alerts_open = atlas_inventory::alerts::list(pool, Some("open"))
        .await?
        .len() as i64;
    atlas_inventory::metrics::record_history(pool, &summary, running, alerts_open).await?;
    atlas_inventory::metrics::prune_history(pool, 48).await?;
    Ok(())
}
