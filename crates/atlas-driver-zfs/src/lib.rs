// Copyright (c) 2026 ZyvorAI Labs Private Limited.
// SPDX-License-Identifier: AGPL-3.0-only OR LicenseRef-Atlas-Commercial
//! ZFS storage driver — a third backend behind the `StorageDriver` trait, showing the pluggable
//! architecture scales past Ceph + NFS.
//!
//! It models a ZFS host as a one-cluster backend: each **zpool** is a `StoragePool`
//! (`kind = "zpool"`) and each pool exposes a **dataset** as a filesystem `StorageVolume`. The MVP
//! reports deterministic capacity fixtures (a real implementation would run `zpool list -Hp` +
//! `zfs list`); the point is a third non-Ceph backend flowing through the same discovery →
//! inventory → REST/gRPC/UI surface.

use async_trait::async_trait;
use atlas_api_types::{
    DiscoveryResult, Health, MetricSample, StorageCluster, StorageHealth, StoragePool,
    StorageVolume, VolumeKind,
};
use atlas_driver_core::{DriverError, StorageDriver};

/// Per-zpool capacity fixture (bytes). A real driver derives these from `zpool list -Hp`.
const POOL_CAPACITY: i64 = 4_000_000_000_000; // 4 TB
const POOL_USED: i64 = 1_600_000_000_000; // 40% used

pub struct ZfsDriver {
    backend_id: String,
    host: String,
    zpools: Vec<String>,
}

impl ZfsDriver {
    pub fn new(
        backend_id: impl Into<String>,
        host: impl Into<String>,
        zpools: Vec<String>,
    ) -> Self {
        Self {
            backend_id: backend_id.into(),
            host: host.into(),
            zpools,
        }
    }

    pub fn with_defaults(backend_id: impl Into<String>) -> Self {
        Self::new(
            backend_id,
            "zfs01.zyvor.lab",
            vec!["tank".into(), "vault".into()],
        )
    }

    fn cluster_id(&self) -> String {
        format!("cls_zfs_{}", sanitize(&self.host))
    }

    fn cluster(&self) -> StorageCluster {
        let n = self.zpools.len().max(1) as i64;
        StorageCluster {
            id: self.cluster_id(),
            backend_id: self.backend_id.clone(),
            name: format!("zfs://{}", self.host),
            native_fsid: None,
            health: Health::Ok,
            raw_capacity_bytes: Some(POOL_CAPACITY * n),
            used_capacity_bytes: Some(POOL_USED * n),
            available_capacity_bytes: Some((POOL_CAPACITY - POOL_USED) * n),
        }
    }

    fn pool_for(&self, zpool: &str) -> StoragePool {
        StoragePool {
            id: format!("pool_zfs_{}", sanitize(zpool)),
            cluster_id: self.cluster_id(),
            name: zpool.to_string(),
            kind: "zpool".into(),
            device_class: None,
            replica_size: None,
            used_bytes: Some(POOL_USED),
            max_bytes: Some(POOL_CAPACITY),
            health: Health::Ok,
        }
    }

    fn pools(&self) -> Vec<StoragePool> {
        self.zpools.iter().map(|z| self.pool_for(z)).collect()
    }
}

fn sanitize(s: &str) -> String {
    s.trim_matches('/')
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect()
}

#[async_trait]
impl StorageDriver for ZfsDriver {
    fn backend_id(&self) -> &str {
        &self.backend_id
    }

    async fn discover(&self) -> Result<DiscoveryResult, DriverError> {
        let mut volumes = Vec::new();
        for z in &self.zpools {
            volumes.extend(self.list_volumes(z).await?);
        }
        Ok(DiscoveryResult {
            cluster: self.cluster(),
            pools: self.pools(),
            osds: vec![],
            volumes,
            health: self.health().await?,
        })
    }

    async fn health(&self) -> Result<StorageHealth, DriverError> {
        let n = self.zpools.len().max(1) as i64;
        Ok(StorageHealth {
            status: Health::Ok,
            summary: format!("{} zpool(s) ONLINE on {}", self.zpools.len(), self.host),
            raw_capacity_bytes: Some(POOL_CAPACITY * n),
            used_capacity_bytes: Some(POOL_USED * n),
            available_capacity_bytes: Some((POOL_CAPACITY - POOL_USED) * n),
            recovering: false,
            degraded_objects: 0,
        })
    }

    async fn list_pools(&self) -> Result<Vec<StoragePool>, DriverError> {
        Ok(self.pools())
    }

    /// One dataset per zpool (a real driver enumerates `zfs list` datasets/zvols).
    async fn list_volumes(&self, pool: &str) -> Result<Vec<StorageVolume>, DriverError> {
        if !self.zpools.iter().any(|z| z == pool) {
            return Ok(vec![]);
        }
        let p = sanitize(pool);
        Ok(vec![StorageVolume {
            id: format!("vol_zfs_{p}"),
            cluster_id: Some(self.cluster_id()),
            pool_id: Some(format!("pool_zfs_{p}")),
            name: format!("{pool}/data"),
            kind: VolumeKind::Filesystem,
            backend_native_id: Some(format!("{}:{}/data", self.host, pool)),
            size_bytes: POOL_CAPACITY,
            used_bytes: Some(POOL_USED),
            state: "available".into(),
            health: Health::Ok,
            kubernetes_namespace: None,
            pvc_name: None,
            storage_class_name: None,
        }])
    }

    async fn metrics(&self) -> Result<Vec<MetricSample>, DriverError> {
        let n = self.zpools.len().max(1) as f64;
        let m = |name: &str, value: f64| MetricSample {
            name: name.into(),
            value,
            labels: Default::default(),
        };
        Ok(vec![
            m("zfs_zpools_total", self.zpools.len() as f64),
            m("zfs_capacity_bytes", POOL_CAPACITY as f64 * n),
            m("zfs_used_bytes", POOL_USED as f64 * n),
        ])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn discover_maps_zpools_to_pools_and_datasets() {
        let d = ZfsDriver::new(
            "bkd_zfs",
            "zfs01.example.com",
            vec!["tank".into(), "vault".into()],
        );
        let r = d.discover().await.unwrap();
        assert_eq!(r.pools.len(), 2);
        assert_eq!(r.pools[0].kind, "zpool");
        assert_eq!(r.volumes.len(), 2);
        assert!(matches!(r.volumes[0].kind, VolumeKind::Filesystem));
        assert_eq!(r.cluster.raw_capacity_bytes, Some(POOL_CAPACITY * 2));
    }

    #[tokio::test]
    async fn unknown_pool_has_no_volumes() {
        let d = ZfsDriver::with_defaults("bkd_zfs");
        assert!(d.list_volumes("nope").await.unwrap().is_empty());
    }
}
