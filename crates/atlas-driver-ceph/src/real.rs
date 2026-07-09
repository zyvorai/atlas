// Copyright (c) 2026 ZyvorAI Labs Private Limited. All rights reserved.
//! Real Ceph driver: parses `ceph`/`rbd` CLI JSON into Atlas DTOs.

use async_trait::async_trait;
use atlas_api_types::{
    DiscoveryResult, Health, MetricSample, Osd, StorageCluster, StorageHealth, StoragePool,
    StorageVolume, VolumeKind,
};
use atlas_driver_core::{DriverError, StorageDriver};

use crate::cmd::{ceph_cmd, rbd_cmd};

/// Live Ceph backend accessed through the `ceph`/`rbd` CLIs.
pub struct RealCephDriver {
    backend_id: String,
}

impl RealCephDriver {
    pub fn new(backend_id: impl Into<String>) -> Self {
        Self {
            backend_id: backend_id.into(),
        }
    }

    async fn cluster_from_status(&self) -> Result<(StorageCluster, StorageHealth), DriverError> {
        let status = ceph_cmd(&["status"]).await?;
        let df = ceph_cmd(&["df", "detail"]).await?;

        let fsid = status
            .get("fsid")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown")
            .to_string();
        let health = map_health(
            status
                .pointer("/health/status")
                .and_then(|v| v.as_str())
                .unwrap_or("HEALTH_UNKNOWN"),
        );

        let stats = df.get("stats");
        let raw = stats
            .and_then(|s| s.get("total_bytes"))
            .and_then(|v| v.as_i64());
        let used = stats
            .and_then(|s| s.get("total_used_bytes"))
            .and_then(|v| v.as_i64());
        let avail = stats
            .and_then(|s| s.get("total_avail_bytes"))
            .and_then(|v| v.as_i64());

        let recovering = status
            .pointer("/pgmap/recovering_bytes_per_sec")
            .and_then(|v| v.as_i64())
            .map(|b| b > 0)
            .unwrap_or(false);
        let degraded = status
            .pointer("/pgmap/degraded_objects")
            .and_then(|v| v.as_i64())
            .unwrap_or(0);
        let summary = status
            .pointer("/health/status")
            .and_then(|v| v.as_str())
            .unwrap_or("HEALTH_UNKNOWN")
            .to_string();

        let cluster = StorageCluster {
            id: format!("cls_{fsid}"),
            backend_id: self.backend_id.clone(),
            name: self.backend_id.clone(),
            native_fsid: Some(fsid),
            health,
            raw_capacity_bytes: raw,
            used_capacity_bytes: used,
            available_capacity_bytes: avail,
        };
        let storage_health = StorageHealth {
            status: health,
            summary,
            raw_capacity_bytes: raw,
            used_capacity_bytes: used,
            available_capacity_bytes: avail,
            recovering,
            degraded_objects: degraded,
        };
        Ok((cluster, storage_health))
    }

    async fn pools_for(&self, cluster_id: &str) -> Result<Vec<StoragePool>, DriverError> {
        let df = ceph_cmd(&["df", "detail"]).await?;
        let empty = vec![];
        let pools = df.get("pools").and_then(|v| v.as_array()).unwrap_or(&empty);
        Ok(pools
            .iter()
            .map(|p| {
                let name = p
                    .get("name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let id = p
                    .get("id")
                    .and_then(|v| v.as_i64())
                    .map(|n| format!("pool_{n}"))
                    .unwrap_or_else(|| format!("pool_{name}"));
                StoragePool {
                    id,
                    cluster_id: cluster_id.to_string(),
                    kind: pool_kind_from_name(&name),
                    name,
                    device_class: None,
                    replica_size: None,
                    used_bytes: p.pointer("/stats/bytes_used").and_then(|v| v.as_i64()),
                    max_bytes: p.pointer("/stats/max_avail").and_then(|v| v.as_i64()),
                    health: Health::Ok,
                }
            })
            .collect())
    }

    async fn osds_for(&self, cluster_id: &str) -> Result<Vec<Osd>, DriverError> {
        let tree = ceph_cmd(&["osd", "tree"]).await?;
        let empty = vec![];
        let nodes = tree
            .get("nodes")
            .and_then(|v| v.as_array())
            .unwrap_or(&empty);
        Ok(nodes
            .iter()
            .filter(|n| n.get("type").and_then(|v| v.as_str()) == Some("osd"))
            .filter_map(|n| {
                let id = n.get("id").and_then(|v| v.as_i64())?;
                let up = n.get("status").and_then(|v| v.as_str()) == Some("up");
                Some(Osd {
                    id,
                    cluster_id: cluster_id.to_string(),
                    up,
                    in_cluster: n
                        .get("reweight")
                        .and_then(|v| v.as_f64())
                        .map(|r| r > 0.0)
                        .unwrap_or(up),
                    device_class: n
                        .get("device_class")
                        .and_then(|v| v.as_str())
                        .map(String::from),
                    host: None,
                    used_bytes: None,
                    capacity_bytes: None,
                })
            })
            .collect())
    }
}

#[async_trait]
impl StorageDriver for RealCephDriver {
    fn backend_id(&self) -> &str {
        &self.backend_id
    }

    async fn discover(&self) -> Result<DiscoveryResult, DriverError> {
        let (cluster, health) = self.cluster_from_status().await?;
        let pools = self.pools_for(&cluster.id).await?;
        let osds = self.osds_for(&cluster.id).await?;

        // Enumerate RBD images in each rbd-kind pool, reconciling each volume's foreign keys to
        // the pool/cluster ids the inventory will store (list_volumes only knows the pool *name*).
        let mut volumes = Vec::new();
        for pool in pools.iter().filter(|p| p.kind == "rbd") {
            let mut vols = self.list_volumes(&pool.name).await.unwrap_or_default();
            for v in &mut vols {
                v.pool_id = Some(pool.id.clone());
                v.cluster_id = Some(cluster.id.clone());
            }
            volumes.extend(vols);
        }

        Ok(DiscoveryResult {
            cluster,
            pools,
            osds,
            volumes,
            health,
        })
    }

    async fn health(&self) -> Result<StorageHealth, DriverError> {
        Ok(self.cluster_from_status().await?.1)
    }

    async fn list_pools(&self) -> Result<Vec<StoragePool>, DriverError> {
        let (cluster, _) = self.cluster_from_status().await?;
        self.pools_for(&cluster.id).await
    }

    async fn list_volumes(&self, pool: &str) -> Result<Vec<StorageVolume>, DriverError> {
        let out = rbd_cmd(&["ls", "-l", pool]).await?;
        let empty = vec![];
        let images = out.as_array().unwrap_or(&empty);
        Ok(images
            .iter()
            // `rbd ls -l` also lists snapshot rows (with a "snapshot" field); keep base images.
            .filter(|img| img.get("snapshot").is_none())
            .filter_map(|img| {
                let name = img.get("image").and_then(|v| v.as_str())?.to_string();
                let size = img.get("size").and_then(|v| v.as_i64()).unwrap_or(0);
                Some(StorageVolume {
                    id: format!("vol_{pool}_{name}"),
                    cluster_id: None,
                    pool_id: Some(format!("pool_{pool}")),
                    name,
                    kind: VolumeKind::Block,
                    backend_native_id: Some(format!("{pool}/{}", img.get("image")?.as_str()?)),
                    size_bytes: size,
                    used_bytes: img.get("used_size").and_then(|v| v.as_i64()),
                    state: "available".into(),
                    health: Health::Ok,
                    kubernetes_namespace: None,
                    pvc_name: None,
                    storage_class_name: None,
                })
            })
            .collect())
    }

    async fn metrics(&self) -> Result<Vec<MetricSample>, DriverError> {
        let (_, h) = self.cluster_from_status().await?;
        let mut out = Vec::new();
        if let Some(v) = h.raw_capacity_bytes {
            out.push(metric("ceph_cluster_total_bytes", v as f64));
        }
        if let Some(v) = h.used_capacity_bytes {
            out.push(metric("ceph_cluster_used_bytes", v as f64));
        }
        if let Some(v) = h.available_capacity_bytes {
            out.push(metric("ceph_cluster_avail_bytes", v as f64));
        }
        out.push(metric("ceph_degraded_objects", h.degraded_objects as f64));
        Ok(out)
    }

    async fn ceph_status(&self) -> Result<serde_json::Value, DriverError> {
        ceph_cmd(&["status"]).await
    }
    async fn ceph_osd_tree(&self) -> Result<serde_json::Value, DriverError> {
        ceph_cmd(&["osd", "tree"]).await
    }
    async fn ceph_df(&self) -> Result<serde_json::Value, DriverError> {
        ceph_cmd(&["df", "detail"]).await
    }
}

fn metric(name: &str, value: f64) -> MetricSample {
    MetricSample {
        name: name.to_string(),
        value,
        labels: Default::default(),
    }
}

fn map_health(s: &str) -> Health {
    match s {
        "HEALTH_OK" => Health::Ok,
        "HEALTH_WARN" => Health::Warn,
        "HEALTH_ERR" => Health::Critical,
        _ => Health::Unknown,
    }
}

/// Heuristic pool classification from its name. A precise mapping would read
/// `ceph osd pool application` metadata; the name hint is sufficient for the MVP inventory view.
fn pool_kind_from_name(name: &str) -> String {
    let n = name.to_lowercase();
    if n.contains("cephfs") && n.contains("meta") {
        "cephfs_metadata".into()
    } else if n.contains("cephfs") {
        "cephfs_data".into()
    } else if n.contains("rgw") || n.starts_with(".rgw") {
        "rgw".into()
    } else if n == "device_health_metrics" || n.starts_with('.') {
        "other".into()
    } else {
        "rbd".into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pool_kind_classification() {
        assert_eq!(pool_kind_from_name("rbd-nvme-prod"), "rbd");
        assert_eq!(pool_kind_from_name("cephfs-data0"), "cephfs_data");
        assert_eq!(pool_kind_from_name("cephfs-metadata"), "cephfs_metadata");
        assert_eq!(pool_kind_from_name(".rgw.root"), "rgw");
    }

    #[test]
    fn health_mapping() {
        assert_eq!(map_health("HEALTH_OK"), Health::Ok);
        assert_eq!(map_health("HEALTH_WARN"), Health::Warn);
        assert_eq!(map_health("HEALTH_ERR"), Health::Critical);
        assert_eq!(map_health("nonsense"), Health::Unknown);
    }
}
