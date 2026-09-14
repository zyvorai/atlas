// Copyright (c) 2026 ZyvorAI Labs Private Limited.
// SPDX-License-Identifier: AGPL-3.0-only OR LicenseRef-Atlas-Commercial
use serde::{Deserialize, Serialize};

/// Ownership reference carried into a create job.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OwnerRef {
    pub product: String,
    pub resource_type: String,
    pub resource_id: String,
    pub role: String,
}

/// The execution payload stored in `storage_jobs.request`, tagged by operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "op")]
pub enum JobSpec {
    #[serde(rename = "volume.create")]
    VolumeCreate {
        volume_id: String,
        backend_id: String,
        name: String,
        namespace: String,
        storage_class: String,
        access_mode: String,
        volume_mode: String,
        size_bytes: i64,
        kind: String,
        policy: Option<String>,
        owner: Option<OwnerRef>,
    },
    #[serde(rename = "volume.delete")]
    VolumeDelete {
        volume_id: String,
        namespace: String,
        pvc_name: String,
    },
    #[serde(rename = "volume.expand")]
    VolumeExpand {
        volume_id: String,
        namespace: String,
        pvc_name: String,
        new_size_bytes: i64,
    },
    #[serde(rename = "snapshot.create")]
    SnapshotCreate {
        snapshot_id: String,
        volume_id: String,
        name: String,
        namespace: String,
        pvc_name: String,
        snapshot_class: String,
    },
    #[serde(rename = "snapshot.delete")]
    SnapshotDelete {
        snapshot_id: String,
        namespace: String,
        name: String,
    },
    /// Provision an RGW bucket via an ObjectBucketClaim and record its endpoint/credentials-ref.
    #[serde(rename = "bucket.create")]
    BucketCreate {
        bucket_id: String,
        namespace: String,
        obc_name: String,
        storage_class: String,
        /// Optional RGW quota (OBC additionalConfig): max object count.
        #[serde(default)]
        max_objects: Option<i64>,
        /// Optional RGW quota (OBC additionalConfig): max size (e.g. "2G").
        #[serde(default)]
        max_size: Option<String>,
    },
    /// Back up a volume: snapshot it and write a manifest to an RGW bucket over S3 (PDF §16).
    #[serde(rename = "backup.create")]
    BackupCreate {
        backup_id: String,
        snapshot_id: String,
        volume_namespace: String,
        pvc_name: String,
        snapshot_name: String,
        snapshot_class: String,
        object_key: String,
        manifest_json: String,
        bucket_namespace: String,
        bucket_secret_ref: String,
        bucket_endpoint: String,
        bucket_name: String,
        bucket_region: String,
        /// "manifest" (default) writes only the metadata manifest; "data" also exports the RBD
        /// image data (`rbd export-diff`) to S3.
        #[serde(default)]
        mode: String,
    },
    /// Clone or restore: provision a new volume (PVC) populated from a VolumeSnapshot.
    #[serde(rename = "snapshot.clone")]
    SnapshotClone {
        /// "clone" (new independent volume) or "restore" (point-in-time copy of the source).
        mode: String,
        new_volume_id: String,
        backend_id: String,
        snapshot_id: String,
        snapshot_k8s_name: String,
        new_name: String,
        namespace: String,
        storage_class: String,
        size_bytes: i64,
        access_mode: String,
        volume_mode: String,
        owner: Option<OwnerRef>,
    },
    /// Restore a volume from a backup: verify the manifest in RGW, then provision a new PVC from
    /// the backup's VolumeSnapshot (PDF §16, DR-2).
    #[serde(rename = "backup.restore")]
    RestoreBackup {
        backup_id: String,
        new_volume_id: String,
        backend_id: String,
        snapshot_id: String,
        snapshot_k8s_name: String,
        new_name: String,
        namespace: String,
        storage_class: String,
        size_bytes: i64,
        object_key: String,
        expected_checksum: String,
        bucket_namespace: String,
        bucket_secret_ref: String,
        bucket_endpoint: String,
        bucket_name: String,
        bucket_region: String,
        /// "snapshot" (default) restores from the CSI VolumeSnapshot; "data" reconstructs the
        /// volume from the RBD diff in S3 (`rbd import-diff`).
        #[serde(default)]
        mode: String,
    },
    /// Provision a raw RBD image directly (bypassing CSI) for non-Kubernetes consumers.
    #[serde(rename = "rbd.create")]
    RbdCreate {
        volume_id: String,
        backend_id: String,
        pool: String,
        image: String,
        size_bytes: i64,
    },
    /// Delete a raw RBD image created via `rbd.create`.
    #[serde(rename = "rbd.delete")]
    RbdDelete {
        volume_id: String,
        pool: String,
        image: String,
    },
    /// Clone a raw RBD image (snapshot + protect + `rbd clone`) into a new COW image.
    #[serde(rename = "rbd.clone")]
    RbdClone {
        volume_id: String,
        backend_id: String,
        pool: String,
        image: String,
        snap: String,
        clone_image: String,
    },
    /// Grow a raw RBD image (`rbd resize`).
    #[serde(rename = "rbd.resize")]
    RbdResize {
        volume_id: String,
        pool: String,
        image: String,
        new_size_bytes: i64,
        /// Day-2: permit a shrink (guarded — can lose data past the new size).
        #[serde(default)]
        allow_shrink: bool,
    },
    /// Flatten a cloned RBD image so it no longer depends on its parent (`rbd flatten`).
    #[serde(rename = "rbd.flatten")]
    RbdFlatten { pool: String, image: String },
    /// Day-2: migrate an RBD image to another pool (`rbd migration` prepare→execute→commit).
    #[serde(rename = "rbd.migrate")]
    RbdMigrate {
        volume_id: String,
        pool: String,
        image: String,
        dest_pool: String,
    },
    /// Day-2 per-image QoS throttle (IOPS / bandwidth caps; `0` clears a cap).
    #[serde(rename = "rbd.qos")]
    RbdQos {
        volume_id: String,
        pool: String,
        image: String,
        iops_limit: Option<i64>,
        bps_limit: Option<i64>,
    },
    /// Day-2 OSD maintenance op: `out` | `in` | `reweight` (weight in [0,1] for reweight).
    #[serde(rename = "ceph.osd.op")]
    CephOsdOp {
        osd_id: i64,
        action: String,
        weight: Option<f64>,
    },
    /// Day-2 DR: RBD mirroring op (`enable` | `disable` | `promote` | `demote`) on a mirrored image.
    #[serde(rename = "rbd.mirror")]
    RbdMirror {
        mirror_id: String,
        pool: String,
        image: String,
        action: String,
        mode: String,
        /// Split-brain promote (`rbd mirror image promote --force`). Only meaningful for `promote`.
        #[serde(default)]
        force: bool,
    },
    /// Snapshot a raw RBD image (`rbd snap create`).
    #[serde(rename = "rbd.snapshot")]
    RbdSnapshot {
        pool: String,
        image: String,
        snap: String,
    },
    /// Roll a raw RBD image back to a snapshot (`rbd snap rollback`). Destructive.
    #[serde(rename = "rbd.rollback")]
    RbdRollback {
        pool: String,
        image: String,
        snap: String,
    },
    /// Delete a raw RBD image's snapshot (unprotect, then `rbd snap rm`). Without this, a
    /// snapshot created through the UI has no path back except deleting the whole parent image.
    #[serde(rename = "rbd.snap_delete")]
    RbdSnapDelete {
        pool: String,
        image: String,
        snap: String,
    },
    /// Delete an RGW bucket: remove its ObjectBucketClaim (Rook releases the bucket) and the row.
    #[serde(rename = "bucket.delete")]
    BucketDelete {
        bucket_id: String,
        namespace: String,
        obc_name: String,
    },
    /// DataBridge: discover a source database's schema (tables/sizes/version/CDC capability).
    #[serde(rename = "databridge.source.discover")]
    SourceDiscover { source_id: String },
    /// DataBridge: score a plan's migration readiness from the discovered schema.
    #[serde(rename = "databridge.assess")]
    MigrationAssess { plan_id: String },
    /// DataBridge: provision the edge database cluster (CloudNativePG / MySQL operator) on Ceph.
    #[serde(rename = "databridge.edge.provision")]
    EdgeDbProvision { plan_id: String },
    /// DataBridge: full-load the source dataset into the edge DB (pg_dump/mydumper).
    #[serde(rename = "databridge.fullload")]
    FullLoad { plan_id: String },
    /// DataBridge: start Debezium CDC from the source to the edge DB.
    #[serde(rename = "databridge.cdc.start")]
    CdcStart { plan_id: String },
    /// DataBridge: stop a plan's CDC stream.
    #[serde(rename = "databridge.cdc.stop")]
    CdcStop { plan_id: String },
    /// DataBridge: re-establish a stalled/errored CDC stream (self-heal).
    #[serde(rename = "databridge.cdc.restart")]
    CdcRestart { plan_id: String },
    /// DataBridge: validate source vs edge (row counts / checksums / schema diff).
    #[serde(rename = "databridge.validate")]
    ValidateRun { plan_id: String, kind: String },
    /// DataBridge: cutover — freeze source, drain CDC, switch endpoint, open rollback window.
    #[serde(rename = "databridge.cutover")]
    Cutover { plan_id: String },
    /// DataBridge: roll back a cutover within its window.
    #[serde(rename = "databridge.rollback")]
    Rollback { plan_id: String },
    /// DataBridge (object leg): copy a cloud object store -> Ceph RGW (full | incremental).
    #[serde(rename = "databridge.object.migrate")]
    ObjectMigrate { migration_id: String },
    /// Delete a backup: remove its S3 manifest + data objects, the RBD snapshot, and the CSI
    /// VolumeSnapshot the backup created (best-effort). The VolumeSnapshot carries the
    /// `pvc-as-source-protection` finalizer — leaving it behind permanently blocks the source
    /// volume from ever actually being deleted (it sits `Terminating` forever), even though Atlas
    /// reports both the backup and a later volume delete as successful.
    #[serde(rename = "backup.delete")]
    BackupDelete {
        backup_id: String,
        manifest_key: String,
        data_key: String,
        volume_namespace: String,
        pvc_name: String,
        rbd_snap: String,
        snapshot_name: String,
        bucket_namespace: String,
        bucket_secret_ref: String,
        bucket_endpoint: String,
        bucket_name: String,
        bucket_region: String,
    },
    /// Create a Rook `CephBlockPool` CR + a matching StorageClass (replicated pools only in this
    /// MVP — erasure-coded pools need k/m data-chunk config not modeled here).
    #[serde(rename = "ceph.pool.create")]
    CephPoolCreate {
        name: String,
        namespace: String,
        storage_class: String,
        replicated_size: i64,
        failure_domain: String,
        device_class: Option<String>,
    },
    /// Delete a `CephBlockPool` CR + its StorageClass.
    #[serde(rename = "ceph.pool.delete")]
    CephPoolDelete {
        name: String,
        namespace: String,
        storage_class: String,
    },
    /// Create a Rook `CephFilesystem` CR (one data pool) + the RWX CephFS StorageClass.
    #[serde(rename = "ceph.filesystem.create")]
    CephFilesystemCreate {
        name: String,
        namespace: String,
        storage_class: String,
        data_pool_name: String,
        replicated_size: i64,
    },
    /// Delete a `CephFilesystem` CR + its StorageClass.
    #[serde(rename = "ceph.filesystem.delete")]
    CephFilesystemDelete {
        name: String,
        namespace: String,
        storage_class: String,
    },
    /// Create a Rook `CephObjectStore` CR + the RGW bucket StorageClass.
    #[serde(rename = "ceph.object_store.create")]
    CephObjectStoreCreate {
        name: String,
        namespace: String,
        storage_class: String,
        replicated_size: i64,
        gateway_port: i64,
        gateway_instances: i64,
    },
    /// Delete a `CephObjectStore` CR + its StorageClass.
    #[serde(rename = "ceph.object_store.delete")]
    CephObjectStoreDelete {
        name: String,
        namespace: String,
        storage_class: String,
    },
}

impl JobSpec {
    pub fn job_type(&self) -> &'static str {
        match self {
            JobSpec::VolumeCreate { .. } => "volume.create",
            JobSpec::VolumeDelete { .. } => "volume.delete",
            JobSpec::VolumeExpand { .. } => "volume.expand",
            JobSpec::SnapshotCreate { .. } => "snapshot.create",
            JobSpec::SnapshotDelete { .. } => "snapshot.delete",
            JobSpec::SnapshotClone { mode, .. } if mode == "restore" => "snapshot.restore",
            JobSpec::SnapshotClone { .. } => "snapshot.clone",
            JobSpec::BucketCreate { .. } => "bucket.create",
            JobSpec::BackupCreate { .. } => "backup.create",
            JobSpec::RestoreBackup { .. } => "backup.restore",
            JobSpec::BackupDelete { .. } => "backup.delete",
            JobSpec::BucketDelete { .. } => "bucket.delete",
            JobSpec::RbdCreate { .. } => "rbd.create",
            JobSpec::RbdDelete { .. } => "rbd.delete",
            JobSpec::RbdClone { .. } => "rbd.clone",
            JobSpec::RbdResize { .. } => "rbd.resize",
            JobSpec::RbdFlatten { .. } => "rbd.flatten",
            JobSpec::RbdMigrate { .. } => "rbd.migrate",
            JobSpec::RbdQos { .. } => "rbd.qos",
            JobSpec::CephOsdOp { .. } => "ceph.osd.op",
            JobSpec::RbdMirror { .. } => "rbd.mirror",
            JobSpec::RbdSnapshot { .. } => "rbd.snapshot",
            JobSpec::RbdRollback { .. } => "rbd.rollback",
            JobSpec::RbdSnapDelete { .. } => "rbd.snap_delete",
            JobSpec::SourceDiscover { .. } => "databridge.source.discover",
            JobSpec::MigrationAssess { .. } => "databridge.assess",
            JobSpec::EdgeDbProvision { .. } => "databridge.edge.provision",
            JobSpec::FullLoad { .. } => "databridge.fullload",
            JobSpec::CdcStart { .. } => "databridge.cdc.start",
            JobSpec::CdcStop { .. } => "databridge.cdc.stop",
            JobSpec::CdcRestart { .. } => "databridge.cdc.restart",
            JobSpec::ValidateRun { .. } => "databridge.validate",
            JobSpec::Cutover { .. } => "databridge.cutover",
            JobSpec::Rollback { .. } => "databridge.rollback",
            JobSpec::ObjectMigrate { .. } => "databridge.object.migrate",
            JobSpec::CephPoolCreate { .. } => "ceph.pool.create",
            JobSpec::CephPoolDelete { .. } => "ceph.pool.delete",
            JobSpec::CephFilesystemCreate { .. } => "ceph.filesystem.create",
            JobSpec::CephFilesystemDelete { .. } => "ceph.filesystem.delete",
            JobSpec::CephObjectStoreCreate { .. } => "ceph.object_store.create",
            JobSpec::CephObjectStoreDelete { .. } => "ceph.object_store.delete",
        }
    }
}
