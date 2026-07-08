// Copyright (c) 2026 ZyvorAI Labs Private Limited. All rights reserved.
//! NFS storage driver — a second backend behind the `StorageDriver` trait, proving Atlas's
//! pluggable-driver architecture beyond Ceph.
//!
//! It models an NFS server as a one-cluster backend: each **export** is a `StoragePool`
//! (`kind = "nfs_export"`) and each mountable **share** under it is a filesystem `StorageVolume`.
//! The MVP resolves exports from configuration and reports deterministic capacity fixtures (a real
//! implementation would shell out to `showmount -e <server>` + `df` on each mount); the point here
//! is that a non-Ceph backend flows through discovery → inventory → the same REST/gRPC surface.

use async_trait::async_trait;
use atlas_api_types::{
    DiscoveryResult, Health, MetricSample, StorageCluster, StorageHealth, StoragePool,
    StorageVolume, VolumeKind,
};
use atlas_driver_core::{DriverError, StorageDriver};

/// Per-export capacity fixture (bytes). A real driver derives these from `df` on the mount.
const EXPORT_CAPACITY: i64 = 8_000_000_000_000; // 8 TB
const EXPORT_USED: i64 = 2_400_000_000_000; // 30% used

pub struct NfsDriver {
    backend_id: String,
    server: String,
    exports: Vec<String>,
}

impl NfsDriver {
    /// Build from an NFS server host and its exported paths.
    pub fn new(
        backend_id: impl Into<String>,
        server: impl Into<String>,
        exports: Vec<String>,
    ) -> Self {
        Self {
            backend_id: backend_id.into(),
            server: server.into(),
            exports,
        }
    }

    /// Demo fixture used when NFS is enabled without an explicit server/exports.
    pub fn with_defaults(backend_id: impl Into<String>) -> Self {
        Self::new(
            backend_id,
            "nfs01.zyvor.lab",
            vec!["/exports/vmstore".into(), "/exports/backups".into()],
        )
    }

    fn cluster_id(&self) -> String {
        format!("cls_nfs_{}", sanitize(&self.server))
    }

    fn cluster(&self) -> StorageCluster {
        let n = self.exports.len().max(1) as i64;
        StorageCluster {
            id: self.cluster_id(),
            backend_id: self.backend_id.clone(),
            name: format!("nfs://{}", self.server),
            native_fsid: None,
            health: Health::Ok,
            raw_capacity_bytes: Some(EXPORT_CAPACITY * n),
            used_capacity_bytes: Some(EXPORT_USED * n),
            available_capacity_bytes: Some((EXPORT_CAPACITY - EXPORT_USED) * n),
        }
    }

    fn pool_for(&self, export: &str) -> StoragePool {
        StoragePool {
            id: format!("pool_nfs_{}", sanitize(export)),
            cluster_id: self.cluster_id(),
            name: export.to_string(),
            kind: "nfs_export".into(),
            device_class: None,
            replica_size: None,
            used_bytes: Some(EXPORT_USED),
            max_bytes: Some(EXPORT_CAPACITY),
            health: Health::Ok,
        }
    }

    fn pools(&self) -> Vec<StoragePool> {
        self.exports.iter().map(|e| self.pool_for(e)).collect()
    }
}

/// Slugify a path/host into an id-safe token.
fn sanitize(s: &str) -> String {
    s.trim_matches('/')
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect()
}

#[async_trait]
impl StorageDriver for NfsDriver {
    fn backend_id(&self) -> &str {
        &self.backend_id
    }

    async fn discover(&self) -> Result<DiscoveryResult, DriverError> {
        let mut volumes = Vec::new();
        for export in &self.exports {
            volumes.extend(self.list_volumes(export).await?);
        }
        Ok(DiscoveryResult {
            cluster: self.cluster(),
            pools: self.pools(),
            osds: vec![], // NFS has no OSDs.
            volumes,
            health: self.health().await?,
        })
    }

    async fn health(&self) -> Result<StorageHealth, DriverError> {
        let n = self.exports.len().max(1) as i64;
        Ok(StorageHealth {
            status: Health::Ok,
            summary: format!(
                "{} export(s) reachable on {}",
                self.exports.len(),
                self.server
            ),
            raw_capacity_bytes: Some(EXPORT_CAPACITY * n),
            used_capacity_bytes: Some(EXPORT_USED * n),
            available_capacity_bytes: Some((EXPORT_CAPACITY - EXPORT_USED) * n),
            recovering: false,
            degraded_objects: 0,
        })
    }

    async fn list_pools(&self) -> Result<Vec<StoragePool>, DriverError> {
        Ok(self.pools())
    }

    /// One mountable filesystem share per export (a real driver enumerates subdirectories).
    async fn list_volumes(&self, pool: &str) -> Result<Vec<StorageVolume>, DriverError> {
        // `pool` is the export path (matches StoragePool.name).
        if !self.exports.iter().any(|e| e == pool) {
            return Ok(vec![]);
        }
        let share = sanitize(pool);
        Ok(vec![StorageVolume {
            id: format!("vol_nfs_{share}"),
            cluster_id: Some(self.cluster_id()),
            pool_id: Some(format!("pool_nfs_{share}")),
            name: format!("{pool}/share"),
            kind: VolumeKind::Filesystem,
            backend_native_id: Some(format!("{}:{}", self.server, pool)),
            size_bytes: EXPORT_CAPACITY,
            used_bytes: Some(EXPORT_USED),
            state: "available".into(),
            health: Health::Ok,
            kubernetes_namespace: None,
            pvc_name: None,
            storage_class_name: None,
        }])
    }

    async fn metrics(&self) -> Result<Vec<MetricSample>, DriverError> {
        let n = self.exports.len().max(1) as f64;
        let m = |name: &str, value: f64| MetricSample {
            name: name.into(),
            value,
            labels: Default::default(),
        };
        Ok(vec![
            m("nfs_exports_total", self.exports.len() as f64),
            m("nfs_capacity_bytes", EXPORT_CAPACITY as f64 * n),
            m("nfs_used_bytes", EXPORT_USED as f64 * n),
        ])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn discover_maps_exports_to_pools_and_volumes() {
        let d = NfsDriver::new(
            "bkd_nfs",
            "nfs01.example.com",
            vec!["/exports/a".into(), "/exports/b".into()],
        );
        let r = d.discover().await.unwrap();
        assert_eq!(r.cluster.backend_id, "bkd_nfs");
        assert_eq!(r.pools.len(), 2);
        assert_eq!(r.pools[0].kind, "nfs_export");
        assert_eq!(r.volumes.len(), 2);
        assert!(matches!(r.volumes[0].kind, VolumeKind::Filesystem));
        assert!(r.osds.is_empty());
        // capacity scales with export count
        assert_eq!(r.cluster.raw_capacity_bytes, Some(EXPORT_CAPACITY * 2));
    }

    #[tokio::test]
    async fn unknown_pool_has_no_volumes() {
        let d = NfsDriver::with_defaults("bkd_nfs");
        assert!(d.list_volumes("/nope").await.unwrap().is_empty());
    }
}
