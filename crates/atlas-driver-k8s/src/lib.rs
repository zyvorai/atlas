// Copyright (c) 2026 ZyvorAI Labs Private Limited. All rights reserved.
//! Live Kubernetes driver (read-only in MVP slice 1).
//!
//! Surfaces `StorageClass`, `PersistentVolumeClaim`, and `PersistentVolume` objects so the gateway
//! can serve `/storage-classes` etc. straight from the cluster. Modeled on
//! `veyron/src/kube/vm_data_disk.rs` (`Api::<StorageClass>::all(...).list(...)`).
//!
//! `create_pvc` is defined but returns `NotImplemented` until MVP slice 2 (the write path).

use atlas_api_types::StorageClassInfo;
use k8s_openapi::api::core::v1::{
    PersistentVolume, PersistentVolumeClaim, PersistentVolumeClaimSpec, TypedLocalObjectReference,
    VolumeResourceRequirements,
};
use k8s_openapi::api::storage::v1::StorageClass;
use k8s_openapi::apimachinery::pkg::api::resource::Quantity;
use k8s_openapi::apimachinery::pkg::apis::meta::v1::ObjectMeta;
use kube::api::{DeleteParams, ListParams, Patch, PatchParams, PostParams};
use kube::core::{ApiResource, DynamicObject, GroupVersionKind};
use kube::{Api, Client};
use std::collections::BTreeMap;

/// Request to create a PVC (the MVP write path).
#[derive(Debug, Clone)]
pub struct PvcCreateSpec {
    pub name: String,
    pub namespace: String,
    pub storage_class: String,
    pub size_bytes: i64,
    pub access_modes: Vec<String>,
    pub volume_mode: Option<String>,
    pub labels: BTreeMap<String, String>,
    /// When set, the PVC is populated from this VolumeSnapshot (clone/restore).
    pub data_source_snapshot: Option<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum K8sError {
    #[error("kube client init failed: {0}")]
    Init(String),
    #[error("kube api error: {0}")]
    Api(#[from] kube::Error),
}

/// Ceph CSI provisioners we badge as Ceph-backed (PDF §7.1).
const CEPH_PROVISIONERS: &[&str] = &[
    "rbd.csi.ceph.com",
    "cephfs.csi.ceph.com",
    "rook-ceph.rbd.csi.ceph.com",
    "rook-ceph.cephfs.csi.ceph.com",
];

fn is_ceph_provisioner(p: &str) -> bool {
    CEPH_PROVISIONERS.iter().any(|c| p.ends_with(c))
}

/// Thin wrapper over a `kube::Client` exposing read-only storage discovery.
#[derive(Clone)]
pub struct K8sDriver {
    client: Client,
}

impl K8sDriver {
    /// Build from the ambient kubeconfig / in-cluster config (honors `KUBECONFIG`).
    pub async fn try_default() -> Result<Self, K8sError> {
        let client = Client::try_default()
            .await
            .map_err(|e| K8sError::Init(e.to_string()))?;
        Ok(Self { client })
    }

    /// List all StorageClasses, tagging Ceph-backed ones.
    pub async fn list_storage_classes(&self) -> Result<Vec<StorageClassInfo>, K8sError> {
        let api: Api<StorageClass> = Api::all(self.client.clone());
        let list = api.list(&ListParams::default().limit(500)).await?;
        Ok(list
            .items
            .into_iter()
            .map(|sc| {
                let provisioner = sc.provisioner;
                StorageClassInfo {
                    is_ceph: is_ceph_provisioner(&provisioner),
                    name: sc.metadata.name.unwrap_or_default(),
                    provisioner,
                    reclaim_policy: sc.reclaim_policy,
                    volume_binding_mode: sc.volume_binding_mode,
                    allow_volume_expansion: sc.allow_volume_expansion,
                    labels: sc.metadata.labels.unwrap_or_default().into_iter().collect(),
                }
            })
            .collect())
    }

    /// List PVCs across all namespaces (or one namespace when `namespace` is `Some`).
    pub async fn list_pvcs(&self, namespace: Option<&str>) -> Result<Vec<PvcSummary>, K8sError> {
        let api: Api<PersistentVolumeClaim> = match namespace {
            Some(ns) => Api::namespaced(self.client.clone(), ns),
            None => Api::all(self.client.clone()),
        };
        let list = api.list(&ListParams::default().limit(500)).await?;
        Ok(list.items.into_iter().map(PvcSummary::from).collect())
    }

    /// List cluster PersistentVolumes.
    pub async fn list_pvs(&self) -> Result<Vec<PvSummary>, K8sError> {
        let api: Api<PersistentVolume> = Api::all(self.client.clone());
        let list = api.list(&ListParams::default().limit(500)).await?;
        Ok(list.items.into_iter().map(PvSummary::from).collect())
    }

    // ---- write path (slice 2) ----

    fn pvc_api(&self, ns: &str) -> Api<PersistentVolumeClaim> {
        Api::namespaced(self.client.clone(), ns)
    }

    /// Create a PVC. Returns its summary (initially Pending until bound).
    pub async fn create_pvc(&self, spec: &PvcCreateSpec) -> Result<PvcSummary, K8sError> {
        let access_modes = if spec.access_modes.is_empty() {
            vec!["ReadWriteOnce".to_string()]
        } else {
            spec.access_modes.clone()
        };
        let mut requests = std::collections::BTreeMap::new();
        requests.insert("storage".to_string(), Quantity(spec.size_bytes.to_string()));

        let pvc = PersistentVolumeClaim {
            metadata: ObjectMeta {
                name: Some(spec.name.clone()),
                namespace: Some(spec.namespace.clone()),
                labels: (!spec.labels.is_empty()).then(|| spec.labels.clone()),
                ..Default::default()
            },
            spec: Some(PersistentVolumeClaimSpec {
                access_modes: Some(access_modes),
                storage_class_name: Some(spec.storage_class.clone()),
                volume_mode: spec.volume_mode.clone(),
                resources: Some(VolumeResourceRequirements {
                    requests: Some(requests),
                    ..Default::default()
                }),
                // Clone/restore: populate from a VolumeSnapshot.
                data_source: spec.data_source_snapshot.as_ref().map(|snap| {
                    TypedLocalObjectReference {
                        api_group: Some("snapshot.storage.k8s.io".to_string()),
                        kind: "VolumeSnapshot".to_string(),
                        name: snap.clone(),
                    }
                }),
                ..Default::default()
            }),
            ..Default::default()
        };
        let created = self
            .pvc_api(&spec.namespace)
            .create(&PostParams::default(), &pvc)
            .await?;
        Ok(PvcSummary::from(created))
    }

    /// Fetch a PVC summary (None if it doesn't exist).
    pub async fn get_pvc(&self, ns: &str, name: &str) -> Result<Option<PvcSummary>, K8sError> {
        match self.pvc_api(ns).get_opt(name).await? {
            Some(p) => Ok(Some(PvcSummary::from(p))),
            None => Ok(None),
        }
    }

    /// Delete a PVC.
    pub async fn delete_pvc(&self, ns: &str, name: &str) -> Result<(), K8sError> {
        self.pvc_api(ns)
            .delete(name, &DeleteParams::default())
            .await?;
        Ok(())
    }

    /// Expand a PVC to a larger size (merge-patch `spec.resources.requests.storage`).
    pub async fn expand_pvc(&self, ns: &str, name: &str, new_bytes: i64) -> Result<(), K8sError> {
        let patch = serde_json::json!({
            "spec": { "resources": { "requests": { "storage": new_bytes.to_string() } } }
        });
        self.pvc_api(ns)
            .patch(name, &PatchParams::default(), &Patch::Merge(&patch))
            .await?;
        Ok(())
    }

    fn volume_snapshot_api(&self, ns: &str) -> (Api<DynamicObject>, ApiResource) {
        let gvk = GroupVersionKind::gvk("snapshot.storage.k8s.io", "v1", "VolumeSnapshot");
        let ar = ApiResource::from_gvk(&gvk);
        (Api::namespaced_with(self.client.clone(), ns, &ar), ar)
    }

    /// Create a VolumeSnapshot from a source PVC. Returns the snapshot object name.
    pub async fn create_volume_snapshot(
        &self,
        ns: &str,
        name: &str,
        source_pvc: &str,
        snapshot_class: &str,
    ) -> Result<String, K8sError> {
        let (api, ar) = self.volume_snapshot_api(ns);
        let mut obj = DynamicObject::new(name, &ar);
        obj.metadata.namespace = Some(ns.to_string());
        obj.data = serde_json::json!({
            "spec": {
                "volumeSnapshotClassName": snapshot_class,
                "source": { "persistentVolumeClaimName": source_pvc }
            }
        });
        let created = api.create(&PostParams::default(), &obj).await?;
        Ok(created.metadata.name.unwrap_or_else(|| name.to_string()))
    }

    /// Whether a VolumeSnapshot reports `status.readyToUse` (None if not yet reported / absent).
    pub async fn volume_snapshot_ready(
        &self,
        ns: &str,
        name: &str,
    ) -> Result<Option<bool>, K8sError> {
        let (api, _ar) = self.volume_snapshot_api(ns);
        match api.get_opt(name).await? {
            Some(obj) => Ok(obj
                .data
                .get("status")
                .and_then(|s| s.get("readyToUse"))
                .and_then(|v| v.as_bool())),
            None => Ok(None),
        }
    }

    /// Delete a VolumeSnapshot.
    pub async fn delete_volume_snapshot(&self, ns: &str, name: &str) -> Result<(), K8sError> {
        let (api, _ar) = self.volume_snapshot_api(ns);
        api.delete(name, &DeleteParams::default()).await?;
        Ok(())
    }
}

/// Compact PVC view for the inventory / API.
#[derive(Debug, Clone, serde::Serialize)]
pub struct PvcSummary {
    pub name: String,
    pub namespace: String,
    pub phase: Option<String>,
    pub storage_class: Option<String>,
    pub volume_name: Option<String>,
    pub requested: Option<String>,
    pub access_modes: Vec<String>,
    pub volume_mode: Option<String>,
}

impl From<PersistentVolumeClaim> for PvcSummary {
    fn from(p: PersistentVolumeClaim) -> Self {
        let spec = p.spec.unwrap_or_default();
        let requested = spec
            .resources
            .and_then(|r| r.requests)
            .and_then(|m| m.get("storage").map(|q| q.0.clone()));
        Self {
            name: p.metadata.name.unwrap_or_default(),
            namespace: p.metadata.namespace.unwrap_or_default(),
            phase: p.status.and_then(|s| s.phase),
            storage_class: spec.storage_class_name,
            volume_name: spec.volume_name,
            requested,
            access_modes: spec.access_modes.unwrap_or_default(),
            volume_mode: spec.volume_mode,
        }
    }
}

/// Compact PV view for the inventory / API.
#[derive(Debug, Clone, serde::Serialize)]
pub struct PvSummary {
    pub name: String,
    pub phase: Option<String>,
    pub storage_class: Option<String>,
    pub capacity: Option<String>,
    pub reclaim_policy: Option<String>,
    pub csi_driver: Option<String>,
}

impl From<PersistentVolume> for PvSummary {
    fn from(p: PersistentVolume) -> Self {
        let spec = p.spec.unwrap_or_default();
        let capacity = spec
            .capacity
            .and_then(|m| m.get("storage").map(|q| q.0.clone()));
        let csi_driver = spec.csi.map(|c| c.driver);
        Self {
            name: p.metadata.name.unwrap_or_default(),
            phase: p.status.and_then(|s| s.phase),
            storage_class: spec.storage_class_name,
            capacity,
            reclaim_policy: spec.persistent_volume_reclaim_policy,
            csi_driver,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ceph_provisioner_detection() {
        assert!(is_ceph_provisioner("rook-ceph.rbd.csi.ceph.com"));
        assert!(is_ceph_provisioner("rbd.csi.ceph.com"));
        assert!(is_ceph_provisioner("cephfs.csi.ceph.com"));
        assert!(!is_ceph_provisioner("nfs.csi.k8s.io"));
        assert!(!is_ceph_provisioner("kubernetes.io/no-provisioner"));
    }
}
