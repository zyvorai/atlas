// Copyright (c) 2026 ZyvorAI Labs Private Limited. All rights reserved.
//! Startup wiring shared by the binary and integration tests: open the DB, register the Ceph
//! backend + driver, optionally attach a live Kubernetes driver, and optionally run an initial
//! discovery pass.

use std::sync::Arc;

use anyhow::{Context, Result};
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
        cordoned: false,
    };
    atlas_inventory::upsert_backend(&pool, &backend).await?;

    let registry = DriverRegistry::new();
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
            cordoned: false,
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
            cordoned: false,
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
    // Durable DB poller (ATLAS_JOB_POLL_SECS) keeps queued work alive across channel loss;
    // tests leave poll_secs=0 so only explicit enqueues/recovery drive the worker.
    let jobs = atlas_jobs::JobEngine::start_with(
        pool.clone(),
        k8s.clone(),
        atlas_jobs::JobEngineOptions {
            poll_secs: config.job_poll_secs,
            stale_secs: config.job_stale_secs,
        },
    );

    // Rate limiter (day-2 governance): per-actor requests/min from ATLAS_RATE_LIMIT_RPM (0 = off).
    let rpm: u32 = std::env::var("ATLAS_RATE_LIMIT_RPM")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);
    let state = AppState {
        pool,
        config: Arc::new(config),
        drivers: Arc::new(registry),
        k8s,
        jobs,
        workers: crate::state::WorkerHealth::default(),
        rate: crate::state::RateLimiter::new(rpm),
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
        // HA leader election: only the leader replica runs the mutating periodic workers, so a
        // multi-replica deployment doesn't double-fire scheduled jobs. On single-replica SQLite this
        // instance always wins. `is_leader` starts false and flips true on the first lease acquire.
        let is_leader = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let lease_ttl = (state.config.monitor_interval_secs as i64 * 3).max(30);
        spawn_leader_election(state.pool.clone(), is_leader.clone(), lease_ttl);

        atlas_monitor::spawn(
            state.pool.clone(),
            driver,
            state.config.monitor_interval_secs,
            state.config.ceph_prometheus_url.clone(),
            state.config.alert_webhook_url.clone(),
            is_leader.clone(),
        );
        // Protection-schedule worker: periodic snapshots + retention (shares the job engine).
        atlas_jobs::spawn_scheduler(
            state.pool.clone(),
            state.jobs.clone(),
            state.config.snapshot_tick_secs,
            is_leader.clone(),
        );
        // Metrics-history sampler: append a capacity/IO/job time-series row each monitor tick,
        // pruning to a 48h window, so the Overview trend charts survive restarts + reloads. It also
        // heartbeats so /readyz can observe the periodic-worker cadence is alive.
        spawn_metrics_sampler(
            state.pool.clone(),
            state.config.monitor_interval_secs,
            state.workers.clone(),
        );
        // DataBridge reconciler: advances migration pipelines (edge CR status, full-load/validation
        // Jobs, CDC lag) that the single-shot job engine can't hold open.
        atlas_databridge::reconcile::spawn_reconciler(
            state.pool.clone(),
            state.k8s.clone(),
            state.config.databridge_reconcile_secs,
            is_leader.clone(),
        );
    }

    // Self-state backup: snapshot the control-plane DB to S3/RGW (independent of the monitor block;
    // disabled unless ATLAS_STATE_BACKUP_SECS > 0).
    spawn_state_backup(state.pool.clone(), state.workers.clone());

    // Audit retention: prune audit rows older than ATLAS_AUDIT_RETENTION_DAYS (0 = keep forever).
    spawn_audit_retention(state.pool.clone());

    Ok(state)
}

/// HA leader election: renew the `workers` lease every `ttl/2`s and flip `is_leader`. Runs on every
/// replica so a non-leader can take over when the current leader's lease expires. The instance id is
/// the pod HOSTNAME (unique per replica in k8s) + pid.
fn spawn_leader_election(
    pool: sqlx::SqlitePool,
    is_leader: std::sync::Arc<std::sync::atomic::AtomicBool>,
    ttl_secs: i64,
) {
    use std::sync::atomic::Ordering;
    let instance = format!(
        "{}-{}",
        std::env::var("HOSTNAME").unwrap_or_else(|_| "gateway".into()),
        std::process::id()
    );
    let renew = (ttl_secs / 2).max(5) as u64;
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(std::time::Duration::from_secs(renew));
        loop {
            tick.tick().await; // fires immediately, so leadership is acquired at startup
            match atlas_inventory::leader::try_acquire(&pool, "workers", &instance, ttl_secs).await {
                Ok(leader) => {
                    let was = is_leader.swap(leader, Ordering::Relaxed);
                    if leader && !was {
                        tracing::info!("acquired worker leadership ({instance})");
                    } else if !leader && was {
                        tracing::warn!("lost worker leadership ({instance})");
                    }
                }
                Err(e) => tracing::warn!("leader election failed: {e:#}"),
            }
        }
    });
}

/// Periodically prune audit rows older than `ATLAS_AUDIT_RETENTION_DAYS` (day-2 governance). Runs
/// every 6h; disabled when the var is unset/0 (keep forever).
fn spawn_audit_retention(pool: sqlx::SqlitePool) {
    let days: i64 = std::env::var("ATLAS_AUDIT_RETENTION_DAYS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);
    if days <= 0 {
        return;
    }
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(std::time::Duration::from_secs(6 * 3600));
        loop {
            tick.tick().await;
            match atlas_inventory::audit::prune(&pool, days).await {
                Ok(n) if n > 0 => tracing::info!("audit retention: pruned {n} row(s) older than {days}d"),
                Ok(_) => {}
                Err(e) => tracing::warn!("audit retention prune failed: {e:#}"),
            }
        }
    });
}

/// Periodically snapshot the control-plane SQLite DB and upload it to S3/RGW so Atlas can restore its
/// own inventory / jobs / audit / quotas / DataBridge state (it backs up tenant volumes but, until
/// now, not itself). Disabled unless `ATLAS_STATE_BACKUP_SECS > 0` and the S3 endpoint/bucket are set.
/// Reads its own env (kept out of `Config` so it doesn't touch every test's `Config` literal).
fn spawn_state_backup(pool: sqlx::SqlitePool, workers: crate::state::WorkerHealth) {
    let secs: u64 = std::env::var("ATLAS_STATE_BACKUP_SECS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);
    if secs == 0 {
        return;
    }
    let endpoint = std::env::var("ATLAS_STATE_BACKUP_ENDPOINT").unwrap_or_default();
    let bucket = std::env::var("ATLAS_STATE_BACKUP_BUCKET").unwrap_or_default();
    if endpoint.is_empty() || bucket.is_empty() {
        tracing::warn!(
            "ATLAS_STATE_BACKUP_SECS is set but ENDPOINT/BUCKET are not — state backup disabled"
        );
        return;
    }
    let access = std::env::var("ATLAS_STATE_BACKUP_ACCESS_KEY").unwrap_or_default();
    let secret = std::env::var("ATLAS_STATE_BACKUP_SECRET_KEY").unwrap_or_default();
    let region = std::env::var("ATLAS_STATE_BACKUP_REGION").unwrap_or_else(|_| "us-east-1".into());
    let prefix = std::env::var("ATLAS_STATE_BACKUP_PREFIX").unwrap_or_else(|_| "atlas-state".into());
    let keep: usize = std::env::var("ATLAS_STATE_BACKUP_KEEP")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(24);

    tokio::spawn(async move {
        let s3 = match atlas_driver_rgw::S3Target::new(&endpoint, &region, &bucket, &access, &secret) {
            Ok(s) => s,
            Err(e) => {
                tracing::error!("state backup disabled: invalid S3 target: {e:#}");
                return;
            }
        };
        tracing::info!(secs, bucket, prefix, keep, "state backup worker started");
        let mut tick = tokio::time::interval(std::time::Duration::from_secs(secs));
        loop {
            tick.tick().await;
            workers.beat("state_backup");
            if let Err(e) = backup_state_once(&pool, &s3, &prefix, keep).await {
                tracing::warn!("state backup failed: {e:#}");
            }
        }
    });
}

/// One state-backup pass: `VACUUM INTO` a consistent snapshot of the live DB, upload it, prune to the
/// newest `keep`. Object keys embed a zero-padded epoch so lexicographic order = chronological order.
async fn backup_state_once(
    pool: &sqlx::SqlitePool,
    s3: &atlas_driver_rgw::S3Target,
    prefix: &str,
    keep: usize,
) -> Result<()> {
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let snap = std::env::temp_dir().join(format!("atlas-state-{ts}.db"));
    // VACUUM INTO writes a transactionally-consistent copy while the DB stays in use.
    sqlx::query(&format!("VACUUM INTO '{}'", snap.display()))
        .execute(pool)
        .await
        .context("VACUUM INTO snapshot")?;
    let bytes = tokio::fs::read(&snap).await.context("read snapshot file")?;
    let _ = tokio::fs::remove_file(&snap).await;
    let size = bytes.len();
    let key = format!("{prefix}/atlas-state-{ts:020}.db");
    s3.put_object(&key, bytes).await.context("upload snapshot")?;
    tracing::info!("state backup: uploaded {key} ({size} bytes)");

    let mut objs = s3
        .list_objects(Some(&format!("{prefix}/")))
        .await
        .unwrap_or_default();
    objs.sort_by(|a, b| a.0.cmp(&b.0));
    if objs.len() > keep {
        for (old, _) in &objs[..objs.len() - keep] {
            let _ = s3.delete_object(old).await;
        }
    }
    Ok(())
}

/// Periodically persist one `metrics_history` sample derived from the current summary + counters.
/// `interval_secs == 0` disables it (mirrors the monitor/scheduler workers).
fn spawn_metrics_sampler(
    pool: sqlx::SqlitePool,
    interval_secs: u64,
    workers: crate::state::WorkerHealth,
) {
    if interval_secs == 0 {
        return;
    }
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(std::time::Duration::from_secs(interval_secs));
        loop {
            tick.tick().await;
            workers.beat("metrics_sampler");
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
