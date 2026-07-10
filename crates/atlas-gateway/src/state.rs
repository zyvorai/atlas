// Copyright (c) 2026 ZyvorAI Labs Private Limited. All rights reserved.
//! Shared application state injected into every axum handler.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use atlas_common::Config;
use atlas_driver_core::DriverRegistry;
use atlas_driver_k8s::K8sDriver;
use sqlx::SqlitePool;

/// In-process worker heartbeats: each periodic worker stamps its name every tick so `/readyz` can
/// surface a wedged worker. Empty when workers are disabled (e.g. tests), which readyz treats as
/// "nothing to check" rather than a failure.
#[derive(Clone, Default)]
pub struct WorkerHealth {
    beats: Arc<Mutex<HashMap<&'static str, Instant>>>,
}

impl WorkerHealth {
    /// Record a heartbeat for `worker` (called at the top of each worker loop iteration).
    pub fn beat(&self, worker: &'static str) {
        if let Ok(mut g) = self.beats.lock() {
            g.insert(worker, Instant::now());
        }
    }

    /// `(worker, seconds_since_last_beat)` for every worker that has beat at least once.
    pub fn ages(&self) -> Vec<(&'static str, u64)> {
        self.beats
            .lock()
            .map(|g| g.iter().map(|(k, v)| (*k, v.elapsed().as_secs())).collect())
            .unwrap_or_default()
    }
}

/// Per-actor fixed-window rate limiter (day-2 governance). `rpm == 0` disables it. Keyed by actor id;
/// each 60s window resets the count. In-process (single-replica); a distributed limiter is a later
/// slice. Configured from `ATLAS_RATE_LIMIT_RPM` at startup.
#[derive(Clone)]
pub struct RateLimiter {
    rpm: u32,
    windows: Arc<Mutex<HashMap<String, (u64, u32)>>>,
}

impl RateLimiter {
    pub fn new(rpm: u32) -> Self {
        Self { rpm, windows: Arc::new(Mutex::new(HashMap::new())) }
    }

    /// Whether the request is allowed. Increments the actor's count for the current minute and
    /// returns false once it exceeds `rpm`. Always true when disabled (`rpm == 0`).
    pub fn allow(&self, actor: &str) -> bool {
        if self.rpm == 0 {
            return true;
        }
        let minute = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() / 60)
            .unwrap_or(0);
        let mut g = match self.windows.lock() {
            Ok(g) => g,
            Err(_) => return true, // never lock out on a poisoned mutex
        };
        let entry = g.entry(actor.to_string()).or_insert((minute, 0));
        if entry.0 != minute {
            *entry = (minute, 0);
        }
        entry.1 += 1;
        entry.1 <= self.rpm
    }
}

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
    /// Periodic-worker heartbeats for `/readyz`.
    pub workers: WorkerHealth,
    /// Per-actor request rate limiter (day-2 governance).
    pub rate: RateLimiter,
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
