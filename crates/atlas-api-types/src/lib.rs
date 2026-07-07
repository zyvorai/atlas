// Copyright (c) 2026 ZyvorAI Labs Private Limited. All rights reserved.
//! Shared serde DTOs for the Atlas storage control plane.
//!
//! These types are the stable wire/domain contract between the gateway, the drivers, and the
//! inventory read model. They intentionally model a *backend-agnostic* storage resource (PDF §3.3):
//! a volume may be backed by Ceph RBD today and SAN/NFS/ZFS/cloud tomorrow.

use serde::{Deserialize, Serialize};

/// Backend technology kind. Ceph is the first driver; the rest are placeholders for future work.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BackendType {
    Ceph,
    Nfs,
    Zfs,
    San,
    CloudBlock,
    /// Kubernetes CSI / StorageClass view (not a physical backend of its own).
    Kubernetes,
}

/// How a backend is operated (PDF §6).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BackendMode {
    ManagedRook,
    External,
    ReadOnly,
}

/// Normalized health for clusters/pools/volumes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Health {
    Ok,
    Warn,
    Critical,
    #[default]
    Unknown,
}

/// The logical kind of a storage volume.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VolumeKind {
    Block,
    Filesystem,
    Object,
}

/// A storage backend registration (row in `storage_backends`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageBackend {
    pub id: String,
    pub name: String,
    pub backend_type: BackendType,
    pub mode: BackendMode,
    pub status: String,
    #[serde(default)]
    pub capabilities: Capabilities,
    /// Secret *reference* only — never a raw keyring/secret (PDF §14.1).
    pub connection_ref: Option<String>,
}

/// Backend capability flags (PDF §12.1).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Capabilities {
    pub block: bool,
    pub file: bool,
    pub object: bool,
    pub snapshots: bool,
    pub clone: bool,
    pub expansion: bool,
    pub replication: bool,
}

/// A discovered storage cluster (Ceph cluster or managed storage cluster).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageCluster {
    pub id: String,
    pub backend_id: String,
    pub name: String,
    pub native_fsid: Option<String>,
    #[serde(default)]
    pub health: Health,
    pub raw_capacity_bytes: Option<i64>,
    pub used_capacity_bytes: Option<i64>,
    pub available_capacity_bytes: Option<i64>,
}

/// A normalized storage pool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoragePool {
    pub id: String,
    pub cluster_id: String,
    pub name: String,
    /// rbd, cephfs_data, cephfs_metadata, rgw, other.
    pub kind: String,
    pub device_class: Option<String>,
    pub replica_size: Option<i64>,
    pub used_bytes: Option<i64>,
    pub max_bytes: Option<i64>,
    #[serde(default)]
    pub health: Health,
}

/// A Zyvor storage volume abstraction.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageVolume {
    pub id: String,
    pub cluster_id: Option<String>,
    pub pool_id: Option<String>,
    pub name: String,
    pub kind: VolumeKind,
    pub backend_native_id: Option<String>,
    pub size_bytes: i64,
    pub used_bytes: Option<i64>,
    pub state: String,
    #[serde(default)]
    pub health: Health,
    pub kubernetes_namespace: Option<String>,
    pub pvc_name: Option<String>,
    pub storage_class_name: Option<String>,
}

/// An OSD entry (Ceph object storage daemon).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Osd {
    pub id: i64,
    pub cluster_id: String,
    pub up: bool,
    pub in_cluster: bool,
    pub device_class: Option<String>,
    pub host: Option<String>,
    pub used_bytes: Option<i64>,
    pub capacity_bytes: Option<i64>,
}

/// Normalized cluster health snapshot.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageHealth {
    #[serde(default)]
    pub status: Health,
    pub summary: String,
    pub raw_capacity_bytes: Option<i64>,
    pub used_capacity_bytes: Option<i64>,
    pub available_capacity_bytes: Option<i64>,
    #[serde(default)]
    pub recovering: bool,
    #[serde(default)]
    pub degraded_objects: i64,
}

/// A single normalized metric sample.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricSample {
    pub name: String,
    pub value: f64,
    #[serde(default)]
    pub labels: std::collections::BTreeMap<String, String>,
}

/// The full result of a driver discovery pass.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscoveryResult {
    pub cluster: StorageCluster,
    pub pools: Vec<StoragePool>,
    pub osds: Vec<Osd>,
    pub volumes: Vec<StorageVolume>,
    pub health: StorageHealth,
}

/// A Kubernetes StorageClass as surfaced by the live k8s driver.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageClassInfo {
    pub name: String,
    pub provisioner: String,
    pub reclaim_policy: Option<String>,
    pub volume_binding_mode: Option<String>,
    pub allow_volume_expansion: Option<bool>,
    /// Ceph-backed classes are tagged so the UI can badge them (PDF §7.1 labels).
    #[serde(default)]
    pub is_ceph: bool,
    #[serde(default)]
    pub labels: std::collections::BTreeMap<String, String>,
}

// ---------------------------------------------------------------------------
// Write-path request types. Defined now for a stable API surface; the driver
// write methods return `NotImplemented` until MVP slice 2.
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateVolumeRequest {
    pub tenant_id: String,
    pub name: String,
    pub size_bytes: i64,
    #[serde(default = "default_block")]
    pub kind: VolumeKind,
    /// Intent policy name (e.g. "production", "database"); resolved by atlas-policy.
    pub policy: Option<String>,
    /// Explicit pool/StorageClass override (bypasses policy placement).
    pub pool: Option<String>,
    /// Product ownership for the resulting volume (recorded in product_bindings).
    #[serde(default)]
    pub owner: Option<Owner>,
    /// Kubernetes provisioning options (the MVP write path creates a PVC).
    #[serde(default)]
    pub kubernetes: Option<K8sVolumeOpts>,
}

fn default_block() -> VolumeKind {
    VolumeKind::Block
}

/// Product ownership of a storage resource (PDF §5.3, §11 product_bindings).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Owner {
    pub product: String,
    pub resource_type: String,
    pub resource_id: String,
    #[serde(default = "default_role")]
    pub role: String,
}

fn default_role() -> String {
    "data_disk".into()
}

/// Kubernetes-specific volume options.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct K8sVolumeOpts {
    pub namespace: Option<String>,
    #[serde(default = "default_true")]
    pub create_pvc: bool,
    #[serde(default)]
    pub access_modes: Vec<String>,
    pub volume_mode: Option<String>,
    /// Explicit StorageClass; otherwise resolved from policy.
    pub storage_class: Option<String>,
}

fn default_true() -> bool {
    true
}

/// A resolved placement decision (atlas-policy output).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Placement {
    pub intent: String,
    pub storage_class: String,
    pub access_mode: String,
    pub volume_mode: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateVolumeResult {
    pub volume_id: String,
    pub backend_native_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExpandVolumeRequest {
    pub volume_id: String,
    pub new_size_bytes: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeleteVolumeRequest {
    pub volume_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateSnapshotRequest {
    pub volume_id: String,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateSnapshotResult {
    pub snapshot_id: String,
    pub backend_native_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CloneSnapshotRequest {
    pub snapshot_id: String,
    pub new_volume_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeleteSnapshotRequest {
    pub snapshot_id: String,
}

// ---------------------------------------------------------------------------
// Jobs & snapshots (read models for slice 2).
// ---------------------------------------------------------------------------

/// Async job states (PDF §10.5).
pub mod job_state {
    pub const PENDING: &str = "pending";
    pub const QUEUED: &str = "queued";
    pub const RUNNING: &str = "running";
    pub const VERIFYING: &str = "verifying";
    pub const SUCCEEDED: &str = "succeeded";
    pub const FAILED: &str = "failed";
}

/// A job record as surfaced by the API.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobRecord {
    pub id: String,
    pub tenant_id: String,
    pub job_type: String,
    pub state: String,
    pub requested_by: String,
    pub progress_percent: i64,
    pub error: Option<String>,
    #[serde(default)]
    pub result: serde_json::Value,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
}

/// A per-tenant storage quota plus current usage (PDF §14 multi-tenancy). `0` limits = unlimited.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TenantQuota {
    pub tenant_id: String,
    /// Max total provisioned volume bytes for the tenant (0 = unlimited).
    pub max_bytes: i64,
    /// Max number of volumes for the tenant (0 = unlimited).
    pub max_volumes: i64,
    /// Current total provisioned bytes across the tenant's volumes.
    pub used_bytes: i64,
    /// Current number of the tenant's volumes.
    pub volume_count: i64,
}

/// An object-storage bucket (RGW/S3) as surfaced by the API.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageBucket {
    pub id: String,
    pub tenant_id: String,
    pub name: String,
    /// The actual bucket name RGW assigned (OBC may generate one).
    pub bucket_name: Option<String>,
    /// S3 endpoint (e.g. http://rook-ceph-rgw-...:80).
    pub endpoint: Option<String>,
    pub region: Option<String>,
    /// Reference to the Kubernetes Secret holding S3 credentials — never the keys (PDF §14.1).
    pub secret_ref: Option<String>,
    /// Namespace of the OBC / Secret / ConfigMap.
    pub namespace: Option<String>,
    pub state: String,
    pub created_at: Option<String>,
}

/// A backup record + manifest pointer (PDF §16.2).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupRecord {
    pub id: String,
    pub tenant_id: String,
    pub volume_id: String,
    pub snapshot_id: Option<String>,
    pub bucket_id: String,
    pub object_key: String,
    pub format: String,
    pub checksum: Option<String>,
    pub state: String,
    pub created_at: Option<String>,
}

/// An alert record (PDF §15.2).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlertRecord {
    pub id: String,
    pub severity: String,
    pub source: String,
    pub resource_type: String,
    pub resource_id: String,
    pub title: String,
    pub description: String,
    #[serde(default)]
    pub evidence: serde_json::Value,
    pub state: String,
    pub created_at: Option<String>,
    pub resolved_at: Option<String>,
}

/// A snapshot record as surfaced by the API.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageSnapshot {
    pub id: String,
    pub tenant_id: String,
    pub volume_id: String,
    pub name: String,
    pub backend_native_id: Option<String>,
    pub consistency: String,
    pub state: String,
    pub protected: bool,
    pub parent_snapshot_id: Option<String>,
    pub created_at: Option<String>,
}
