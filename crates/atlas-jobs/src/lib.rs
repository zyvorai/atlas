// Copyright (c) 2026 ZyvorAI Labs Private Limited. All rights reserved.
//! In-process async job engine backed by the `storage_jobs` table (PDF Rule 1: every storage
//! operation is a job; §10.5 state machine). A single tokio worker consumes job ids from an
//! unbounded channel, executes them against the Kubernetes driver + policy, and drives the job
//! through `pending → queued → running → verifying → succeeded | failed`.
//!
//! This covers the `atlas-provisioner` + `atlas-snapshot` responsibilities from the plan without a
//! Redis/NATS dependency (we are SQLite-only). It can be swapped for a durable queue later without
//! changing the gateway.

use std::sync::Arc;
use std::time::Duration;

use anyhow::{anyhow, Context, Result};
use atlas_api_types::{Health, JobRecord, StorageVolume, VolumeKind};
use atlas_driver_k8s::{K8sDriver, PvcCreateSpec};
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use tokio::sync::mpsc;

/// How long to wait for a PVC to bind / a snapshot to become ready before returning
/// (the resource still exists; we just stop polling).
const BIND_TIMEOUT: Duration = Duration::from_secs(45);
const POLL_INTERVAL: Duration = Duration::from_secs(2);

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
    /// Delete an RGW bucket: remove its ObjectBucketClaim (Rook releases the bucket) and the row.
    #[serde(rename = "bucket.delete")]
    BucketDelete {
        bucket_id: String,
        namespace: String,
        obc_name: String,
    },
    /// Delete a backup: remove its S3 manifest + data objects and the RBD snapshot (best-effort).
    #[serde(rename = "backup.delete")]
    BackupDelete {
        backup_id: String,
        manifest_key: String,
        data_key: String,
        volume_namespace: String,
        pvc_name: String,
        rbd_snap: String,
        bucket_namespace: String,
        bucket_secret_ref: String,
        bucket_endpoint: String,
        bucket_name: String,
        bucket_region: String,
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
        }
    }
}

/// Handle used by the gateway to enqueue jobs.
#[derive(Clone)]
pub struct JobEngine {
    pool: SqlitePool,
    tx: mpsc::UnboundedSender<String>,
}

impl JobEngine {
    /// Start the engine: spawn the worker task and return a cloneable handle.
    pub fn start(pool: SqlitePool, k8s: Option<Arc<K8sDriver>>) -> Self {
        let (tx, rx) = mpsc::unbounded_channel::<String>();
        let worker_pool = pool.clone();
        tokio::spawn(async move { run_worker(worker_pool, k8s, rx).await });
        Self { pool, tx }
    }

    /// Enqueue a job. Honors idempotency: a repeated key returns the existing job (PDF §17.4).
    pub async fn enqueue(
        &self,
        job_id: &str,
        tenant_id: &str,
        requested_by: &str,
        spec: JobSpec,
        idempotency_key: Option<&str>,
    ) -> Result<JobRecord> {
        if let Some(key) = idempotency_key {
            if let Some(existing) =
                atlas_inventory::jobs::find_by_idempotency(&self.pool, key).await?
            {
                return Ok(existing);
            }
        }
        let request = serde_json::to_value(&spec)?;
        atlas_inventory::jobs::insert_job(
            &self.pool,
            job_id,
            tenant_id,
            spec.job_type(),
            requested_by,
            &request,
            idempotency_key,
        )
        .await?;
        atlas_inventory::jobs::set_state(&self.pool, job_id, "queued", 0).await?;
        // If the worker channel is closed the job stays queued; surface that.
        self.tx
            .send(job_id.to_string())
            .map_err(|_| anyhow!("job worker is not running"))?;
        atlas_inventory::jobs::get_job(&self.pool, job_id)
            .await?
            .ok_or_else(|| anyhow!("job disappeared after insert"))
    }
}

/// The worker loop: one job at a time (storage ops are cheap to serialize and this keeps ordering
/// simple). A failing job is marked `failed`; it never takes down the worker.
async fn run_worker(
    pool: SqlitePool,
    k8s: Option<Arc<K8sDriver>>,
    mut rx: mpsc::UnboundedReceiver<String>,
) {
    tracing::info!("atlas-jobs worker started");
    while let Some(job_id) = rx.recv().await {
        if let Err(e) = execute_job(&pool, &k8s, &job_id).await {
            tracing::warn!(job = %job_id, "job failed: {e:#}");
            let _ = atlas_inventory::jobs::mark_failed(&pool, &job_id, &format!("{e:#}")).await;
        }
    }
    tracing::warn!("atlas-jobs worker channel closed");
}

async fn execute_job(pool: &SqlitePool, k8s: &Option<Arc<K8sDriver>>, job_id: &str) -> Result<()> {
    let job = atlas_inventory::jobs::get_job(pool, job_id)
        .await?
        .ok_or_else(|| anyhow!("job {job_id} not found"))?;
    // Reload the raw request payload (get_job doesn't return it).
    let request = load_request(pool, job_id).await?;
    let spec: JobSpec = serde_json::from_value(request).context("decode job request")?;

    atlas_inventory::jobs::mark_running(pool, job_id).await?;
    let result = dispatch(pool, k8s, &job.tenant_id, spec).await?;
    atlas_inventory::jobs::mark_succeeded(pool, job_id, &result).await?;
    Ok(())
}

async fn load_request(pool: &SqlitePool, job_id: &str) -> Result<serde_json::Value> {
    let s: String = sqlx::query_scalar("SELECT request FROM storage_jobs WHERE id=?")
        .bind(job_id)
        .fetch_one(pool)
        .await?;
    Ok(serde_json::from_str(&s)?)
}

async fn dispatch(
    pool: &SqlitePool,
    k8s: &Option<Arc<K8sDriver>>,
    tenant_id: &str,
    spec: JobSpec,
) -> Result<serde_json::Value> {
    match spec {
        JobSpec::RbdCreate {
            volume_id,
            backend_id,
            pool: rbd_pool,
            image,
            size_bytes,
        } => {
            atlas_driver_ceph::rbd_create(&rbd_pool, &image, size_bytes)
                .await
                .with_context(|| format!("rbd create {rbd_pool}/{image}"))?;
            let vol = StorageVolume {
                id: volume_id.clone(),
                cluster_id: None,
                pool_id: None,
                name: image.clone(),
                kind: VolumeKind::Block,
                backend_native_id: Some(format!("rbd:{rbd_pool}/{image}")),
                size_bytes,
                used_bytes: None,
                state: "available".into(),
                health: Health::Ok,
                kubernetes_namespace: None,
                pvc_name: None,
                storage_class_name: None,
            };
            atlas_inventory::upsert_volume(pool, &backend_id, tenant_id, &vol, None).await?;
            Ok(serde_json::json!({
                "volume_id": volume_id, "rbd": format!("{rbd_pool}/{image}"),
                "size_bytes": size_bytes, "state": "available"
            }))
        }
        JobSpec::RbdDelete {
            volume_id,
            pool: rbd_pool,
            image,
        } => {
            atlas_driver_ceph::rbd_remove(&rbd_pool, &image)
                .await
                .with_context(|| format!("rbd rm {rbd_pool}/{image}"))?;
            atlas_inventory::delete_volume_row(pool, &volume_id).await?;
            Ok(serde_json::json!({
                "volume_id": volume_id, "rbd": format!("{rbd_pool}/{image}"), "deleted": true
            }))
        }
        JobSpec::RbdClone {
            volume_id,
            backend_id,
            pool: rbd_pool,
            image,
            snap,
            clone_image,
        } => {
            // Snapshot the source, protect it (required for cloning), then create the COW clone.
            atlas_driver_ceph::rbd_snap_create(&rbd_pool, &image, &snap)
                .await
                .with_context(|| format!("rbd snap create {rbd_pool}/{image}@{snap}"))?;
            atlas_driver_ceph::rbd_snap_protect(&rbd_pool, &image, &snap)
                .await
                .with_context(|| format!("rbd snap protect {rbd_pool}/{image}@{snap}"))?;
            atlas_driver_ceph::rbd_clone(&rbd_pool, &image, &snap, &rbd_pool, &clone_image)
                .await
                .with_context(|| format!("rbd clone {rbd_pool}/{image}@{snap} -> {clone_image}"))?;
            let size_bytes = atlas_driver_ceph::rbd_info_size(&rbd_pool, &clone_image)
                .await
                .unwrap_or(0);
            let vol = StorageVolume {
                id: volume_id.clone(),
                cluster_id: None,
                pool_id: None,
                name: clone_image.clone(),
                kind: VolumeKind::Block,
                backend_native_id: Some(format!("rbd:{rbd_pool}/{clone_image}")),
                size_bytes,
                used_bytes: None,
                state: "available".into(),
                health: Health::Ok,
                kubernetes_namespace: None,
                pvc_name: None,
                storage_class_name: None,
            };
            atlas_inventory::upsert_volume(pool, &backend_id, tenant_id, &vol, None).await?;
            Ok(serde_json::json!({
                "volume_id": volume_id, "clone": format!("{rbd_pool}/{clone_image}"),
                "parent": format!("{rbd_pool}/{image}@{snap}"), "size_bytes": size_bytes
            }))
        }
        JobSpec::VolumeCreate {
            volume_id,
            backend_id,
            name,
            namespace,
            storage_class,
            access_mode,
            volume_mode,
            size_bytes,
            kind,
            policy,
            owner,
        } => {
            let k8s = require_k8s(k8s)?;
            let mut labels = std::collections::BTreeMap::new();
            labels.insert("zyvor.dev/volume-id".to_string(), volume_id.clone());
            if let Some(o) = &owner {
                labels.insert("zyvor.dev/owner-product".to_string(), o.product.clone());
            }
            let create = PvcCreateSpec {
                name: name.clone(),
                namespace: namespace.clone(),
                storage_class: storage_class.clone(),
                size_bytes,
                access_modes: vec![access_mode],
                volume_mode: Some(volume_mode),
                labels,
                data_source_snapshot: None,
            };
            k8s.create_pvc(&create)
                .await
                .with_context(|| format!("create PVC {namespace}/{name}"))?;

            // Verify: poll for Bound (Immediate SCs bind quickly; WaitForFirstConsumer stays Pending).
            let phase = poll_pvc_phase(&k8s, &namespace, &name).await;
            let vol = StorageVolume {
                id: volume_id.clone(),
                cluster_id: None,
                pool_id: None,
                name: name.clone(),
                kind: parse_kind(&kind),
                backend_native_id: Some(format!("pvc/{namespace}/{name}")),
                size_bytes,
                used_bytes: None,
                state: phase
                    .clone()
                    .unwrap_or_else(|| "provisioning".into())
                    .to_lowercase(),
                health: if phase.as_deref() == Some("Bound") {
                    Health::Ok
                } else {
                    Health::Unknown
                },
                kubernetes_namespace: Some(namespace.clone()),
                pvc_name: Some(name.clone()),
                storage_class_name: Some(storage_class),
            };
            atlas_inventory::upsert_volume(pool, &backend_id, tenant_id, &vol, policy.as_deref())
                .await?;

            if let Some(o) = owner {
                atlas_inventory::insert_binding(
                    pool,
                    &format!("bind_{volume_id}"),
                    tenant_id,
                    &o.product,
                    &o.resource_type,
                    &o.resource_id,
                    "volume",
                    &volume_id,
                    &o.role,
                )
                .await?;
            }
            Ok(serde_json::json!({
                "volume_id": volume_id, "pvc": format!("{namespace}/{name}"),
                "phase": phase, "bound": phase.as_deref() == Some("Bound")
            }))
        }

        JobSpec::VolumeDelete {
            volume_id,
            namespace,
            pvc_name,
        } => {
            let k8s = require_k8s(k8s)?;
            atlas_inventory::set_volume_state(pool, &volume_id, "deleting").await?;
            // Deleting an absent PVC is not an error (idempotent).
            if let Err(e) = k8s.delete_pvc(&namespace, &pvc_name).await {
                tracing::warn!("delete_pvc {namespace}/{pvc_name}: {e}");
            }
            atlas_inventory::delete_volume_row(pool, &volume_id).await?;
            Ok(serde_json::json!({ "volume_id": volume_id, "deleted": true }))
        }

        JobSpec::VolumeExpand {
            volume_id,
            namespace,
            pvc_name,
            new_size_bytes,
        } => {
            let k8s = require_k8s(k8s)?;
            k8s.expand_pvc(&namespace, &pvc_name, new_size_bytes)
                .await
                .with_context(|| format!("expand PVC {namespace}/{pvc_name}"))?;
            sqlx::query(
                "UPDATE storage_volumes SET size_bytes=?, updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE id=?",
            )
            .bind(new_size_bytes)
            .bind(&volume_id)
            .execute(pool)
            .await?;
            Ok(serde_json::json!({ "volume_id": volume_id, "size_bytes": new_size_bytes }))
        }

        JobSpec::SnapshotCreate {
            snapshot_id,
            volume_id,
            name,
            namespace,
            pvc_name,
            snapshot_class,
        } => {
            let k8s = require_k8s(k8s)?;
            atlas_inventory::snapshots::insert_snapshot(
                pool,
                &snapshot_id,
                tenant_id,
                &volume_id,
                &name,
                None,
                "crash",
                "creating",
            )
            .await?;
            k8s.create_volume_snapshot(&namespace, &name, &pvc_name, &snapshot_class)
                .await
                .with_context(|| format!("create VolumeSnapshot {namespace}/{name}"))?;
            let ready = poll_snapshot_ready(&k8s, &namespace, &name).await;
            atlas_inventory::snapshots::set_state(
                pool,
                &snapshot_id,
                if ready { "ready" } else { "creating" },
            )
            .await?;
            Ok(serde_json::json!({ "snapshot_id": snapshot_id, "ready": ready }))
        }

        JobSpec::SnapshotDelete {
            snapshot_id,
            namespace,
            name,
        } => {
            let k8s = require_k8s(k8s)?;
            if let Err(e) = k8s.delete_volume_snapshot(&namespace, &name).await {
                tracing::warn!("delete VolumeSnapshot {namespace}/{name}: {e}");
            }
            atlas_inventory::snapshots::delete_snapshot_row(pool, &snapshot_id).await?;
            Ok(serde_json::json!({ "snapshot_id": snapshot_id, "deleted": true }))
        }

        JobSpec::SnapshotClone {
            mode,
            new_volume_id,
            backend_id,
            snapshot_id,
            snapshot_k8s_name,
            new_name,
            namespace,
            storage_class,
            size_bytes,
            access_mode,
            volume_mode,
            owner,
        } => {
            let k8s = require_k8s(k8s)?;
            let phase = provision_from_snapshot(
                pool,
                &k8s,
                tenant_id,
                &backend_id,
                &new_volume_id,
                &snapshot_id,
                &snapshot_k8s_name,
                &new_name,
                &namespace,
                &storage_class,
                size_bytes,
                &access_mode,
                &volume_mode,
                &mode,
                owner,
            )
            .await?;
            Ok(serde_json::json!({
                "volume_id": new_volume_id, "from_snapshot": snapshot_id, "mode": mode,
                "pvc": format!("{namespace}/{new_name}"),
                "phase": phase, "bound": phase.as_deref() == Some("Bound")
            }))
        }

        JobSpec::RestoreBackup {
            backup_id,
            new_volume_id,
            backend_id,
            snapshot_id,
            snapshot_k8s_name,
            new_name,
            namespace,
            storage_class,
            size_bytes,
            object_key,
            expected_checksum,
            bucket_namespace,
            bucket_secret_ref,
            bucket_endpoint,
            bucket_name,
            bucket_region,
            mode,
        } => {
            let k8s = require_k8s(k8s)?;
            // 1. Read the backup manifest back from RGW and verify its checksum (integrity check).
            let manifest_bytes = read_backup_manifest(
                &k8s,
                &bucket_namespace,
                &bucket_secret_ref,
                &bucket_endpoint,
                &bucket_region,
                &bucket_name,
                &object_key,
            )
            .await
            .ok();
            let manifest_verified = manifest_bytes
                .as_ref()
                .map(|b| sha256_hex(b) == expected_checksum)
                .unwrap_or(false);
            if !manifest_verified {
                tracing::warn!("backup {backup_id} manifest checksum mismatch/unreadable");
            }
            let manifest: serde_json::Value = manifest_bytes
                .as_ref()
                .and_then(|b| serde_json::from_slice(b).ok())
                .unwrap_or_else(|| serde_json::json!({}));

            // 2a. Data restore: create an empty PVC and apply the RBD diff from S3.
            if mode == "data" {
                let data_object = manifest
                    .pointer("/data/data_object")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| {
                        anyhow!("backup {backup_id} has no data object (not a data-mode backup)")
                    })?
                    .to_string();
                let data_checksum = manifest
                    .pointer("/data/data_checksum")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default()
                    .to_string();

                let mut labels = std::collections::BTreeMap::new();
                labels.insert("zyvor.dev/volume-id".to_string(), new_volume_id.clone());
                labels.insert("zyvor.dev/restored-from".to_string(), backup_id.clone());
                let create = PvcCreateSpec {
                    name: new_name.clone(),
                    namespace: namespace.clone(),
                    storage_class: storage_class.clone(),
                    size_bytes,
                    access_modes: vec!["ReadWriteOnce".into()],
                    volume_mode: Some("Filesystem".into()),
                    labels,
                    data_source_snapshot: None,
                };
                k8s.create_pvc(&create)
                    .await
                    .with_context(|| format!("create restore PVC {namespace}/{new_name}"))?;
                let phase = poll_pvc_phase(&k8s, &namespace, &new_name).await;
                if phase.as_deref() != Some("Bound") {
                    anyhow::bail!(
                        "restore PVC {namespace}/{new_name} did not bind (phase {phase:?})"
                    );
                }
                let (pool_name, image) = k8s
                    .resolve_rbd(&namespace, &new_name)
                    .await?
                    .ok_or_else(|| anyhow!("could not resolve RBD image for restored PVC"))?;

                // Stream the S3 data object straight into `rbd import-diff` — no in-memory buffer.
                let s3 = build_s3_target(
                    &k8s,
                    &bucket_namespace,
                    &bucket_secret_ref,
                    &bucket_endpoint,
                    &bucket_region,
                    &bucket_name,
                )
                .await?;
                let mut child = atlas_driver_ceph::rbd_import_diff_child(&pool_name, &image)
                    .map_err(|e| anyhow!("{e}"))?;
                let stdin = child
                    .stdin
                    .take()
                    .ok_or_else(|| anyhow!("rbd import-diff: no stdin"))?;
                let dl = s3.get_object_streaming(&data_object, stdin).await;
                let (imported_bytes, streamed_checksum) = match dl {
                    Ok(v) => v,
                    Err(e) => {
                        let _ = child.kill().await;
                        return Err(e.context(format!("stream backup data {data_object}")));
                    }
                };
                // `stdin` was moved into get_object_streaming and dropped there, closing the pipe.
                let status = child.wait().await.context("wait for rbd import-diff")?;
                if !status.success() {
                    let mut err = String::new();
                    if let Some(mut se) = child.stderr.take() {
                        use tokio::io::AsyncReadExt;
                        let _ = se.read_to_string(&mut err).await;
                    }
                    anyhow::bail!("rbd import-diff into {pool_name}/{image}: {}", err.trim());
                }
                let data_verified = streamed_checksum == data_checksum;
                if !data_verified {
                    tracing::warn!("backup {backup_id} data checksum mismatch");
                }
                let imported_bytes = imported_bytes as usize;

                let vol = StorageVolume {
                    id: new_volume_id.clone(),
                    cluster_id: None,
                    pool_id: None,
                    name: new_name.clone(),
                    kind: VolumeKind::Block,
                    backend_native_id: Some(format!("pvc/{namespace}/{new_name}")),
                    size_bytes,
                    used_bytes: None,
                    state: "bound".into(),
                    health: Health::Ok,
                    kubernetes_namespace: Some(namespace.clone()),
                    pvc_name: Some(new_name.clone()),
                    storage_class_name: Some(storage_class.clone()),
                };
                atlas_inventory::upsert_volume(pool, &backend_id, tenant_id, &vol, None).await?;
                return Ok(serde_json::json!({
                    "volume_id": new_volume_id, "from_backup": backup_id, "mode": "data",
                    "manifest_verified": manifest_verified, "data_verified": data_verified,
                    "imported_bytes": imported_bytes, "rbd": format!("{pool_name}/{image}"),
                    "pvc": format!("{namespace}/{new_name}"), "bound": true
                }));
            }

            // 2b. Snapshot restore (default): provision a new volume from the backup's VolumeSnapshot.
            let phase = provision_from_snapshot(
                pool,
                &k8s,
                tenant_id,
                &backend_id,
                &new_volume_id,
                &snapshot_id,
                &snapshot_k8s_name,
                &new_name,
                &namespace,
                &storage_class,
                size_bytes,
                "ReadWriteOnce",
                "Filesystem",
                "restore-backup",
                None,
            )
            .await?;
            Ok(serde_json::json!({
                "volume_id": new_volume_id, "from_backup": backup_id, "mode": "snapshot",
                "manifest_verified": manifest_verified,
                "pvc": format!("{namespace}/{new_name}"),
                "phase": phase, "bound": phase.as_deref() == Some("Bound")
            }))
        }

        JobSpec::BackupDelete {
            backup_id,
            manifest_key,
            data_key,
            volume_namespace,
            pvc_name,
            rbd_snap,
            bucket_namespace,
            bucket_secret_ref,
            bucket_endpoint,
            bucket_name,
            bucket_region,
        } => {
            let k8s = require_k8s(k8s)?;
            let secret = k8s
                .get_secret(&bucket_namespace, &bucket_secret_ref)
                .await?
                .ok_or_else(|| anyhow!("bucket secret {bucket_secret_ref} not found"))?;
            let access = secret
                .get("AWS_ACCESS_KEY_ID")
                .ok_or_else(|| anyhow!("bucket secret missing AWS_ACCESS_KEY_ID"))?;
            let secret_key = secret
                .get("AWS_SECRET_ACCESS_KEY")
                .ok_or_else(|| anyhow!("bucket secret missing AWS_SECRET_ACCESS_KEY"))?;
            let s3 = atlas_driver_rgw::S3Target::new(
                &bucket_endpoint,
                &bucket_region,
                &bucket_name,
                access,
                secret_key,
            )?;
            // Remove the S3 objects (idempotent).
            if let Err(e) = s3.delete_object(&manifest_key).await {
                tracing::warn!("delete manifest {manifest_key}: {e:#}");
            }
            if !data_key.is_empty() {
                if let Err(e) = s3.delete_object(&data_key).await {
                    tracing::warn!("delete data {data_key}: {e:#}");
                }
            }
            // Best-effort: remove the RBD snapshot if the source volume still resolves.
            if let Ok(Some((pool_name, image))) =
                k8s.resolve_rbd(&volume_namespace, &pvc_name).await
            {
                if let Err(e) = atlas_driver_ceph::rbd_snap_rm(&pool_name, &image, &rbd_snap).await
                {
                    tracing::warn!("rbd snap rm {pool_name}/{image}@{rbd_snap}: {e:#}");
                }
            }
            atlas_inventory::backups::delete_backup_row(pool, &backup_id).await?;
            Ok(serde_json::json!({ "backup_id": backup_id, "deleted": true }))
        }

        JobSpec::BucketDelete {
            bucket_id,
            namespace,
            obc_name,
        } => {
            let k8s = require_k8s(k8s)?;
            if let Err(e) = k8s.delete_obc(&namespace, &obc_name).await {
                tracing::warn!("delete OBC {namespace}/{obc_name}: {e}");
            }
            atlas_inventory::buckets::delete_bucket_row(pool, &bucket_id).await?;
            Ok(serde_json::json!({ "bucket_id": bucket_id, "deleted": true }))
        }

        JobSpec::BucketCreate {
            bucket_id,
            namespace,
            obc_name,
            storage_class,
            max_objects,
            max_size,
        } => {
            let k8s = require_k8s(k8s)?;
            let max_size = max_size.filter(|s| !s.is_empty());
            let mut additional_config = std::collections::BTreeMap::new();
            if let Some(n) = max_objects {
                additional_config.insert("maxObjects".to_string(), n.to_string());
            }
            if let Some(sz) = &max_size {
                additional_config.insert("maxSize".to_string(), sz.clone());
            }
            k8s.create_obc(&namespace, &obc_name, &storage_class, &additional_config)
                .await
                .with_context(|| format!("create OBC {namespace}/{obc_name}"))?;

            // Rook writes a ConfigMap (same name) with BUCKET_* once the OBC binds.
            let cm = poll_configmap(&k8s, &namespace, &obc_name)
                .await
                .ok_or_else(|| anyhow!("OBC {obc_name} did not bind (no ConfigMap) in time"))?;
            let bucket_name = cm.get("BUCKET_NAME").cloned().unwrap_or_default();
            let host = cm.get("BUCKET_HOST").cloned().unwrap_or_default();
            let port = cm
                .get("BUCKET_PORT")
                .cloned()
                .unwrap_or_else(|| "80".into());
            let region = cm
                .get("BUCKET_REGION")
                .filter(|r| !r.is_empty())
                .cloned()
                .unwrap_or_else(|| "us-east-1".into());
            let endpoint = format!("http://{host}:{port}");
            // The OBC Secret (same name) holds the S3 credentials; store only the reference.
            atlas_inventory::buckets::set_bound(
                pool,
                &bucket_id,
                &bucket_name,
                &endpoint,
                &region,
                &obc_name,
            )
            .await?;

            // Enforce the RGW per-bucket quota directly (Rook may not apply OBC additionalConfig).
            let quota_set = if max_objects.is_some() || max_size.is_some() {
                match atlas_driver_ceph::radosgw_bucket_quota(
                    &bucket_name,
                    max_objects,
                    max_size.as_deref(),
                )
                .await
                {
                    Ok(()) => true,
                    Err(e) => {
                        tracing::warn!("set bucket quota on {bucket_name}: {e:#}");
                        false
                    }
                }
            } else {
                false
            };
            Ok(serde_json::json!({
                "bucket_id": bucket_id, "bucket_name": bucket_name, "endpoint": endpoint,
                "quota_set": quota_set
            }))
        }

        JobSpec::BackupCreate {
            backup_id,
            snapshot_id,
            volume_namespace,
            pvc_name,
            snapshot_name,
            snapshot_class,
            object_key,
            manifest_json,
            bucket_namespace,
            bucket_secret_ref,
            bucket_endpoint,
            bucket_name,
            bucket_region,
            mode,
        } => {
            let k8s = require_k8s(k8s)?;
            // 1. point-in-time CSI snapshot of the source volume.
            k8s.create_volume_snapshot(
                &volume_namespace,
                &snapshot_name,
                &pvc_name,
                &snapshot_class,
            )
            .await
            .with_context(|| format!("snapshot {volume_namespace}/{snapshot_name} for backup"))?;
            let ready = poll_snapshot_ready(&k8s, &volume_namespace, &snapshot_name).await;
            atlas_inventory::snapshots::set_state(
                pool,
                &snapshot_id,
                if ready { "ready" } else { "creating" },
            )
            .await?;

            // 2. read the bucket's S3 credentials in-cluster (never logged).
            let secret = k8s
                .get_secret(&bucket_namespace, &bucket_secret_ref)
                .await?
                .ok_or_else(|| anyhow!("bucket secret {bucket_secret_ref} not found"))?;
            let access = secret
                .get("AWS_ACCESS_KEY_ID")
                .ok_or_else(|| anyhow!("bucket secret missing AWS_ACCESS_KEY_ID"))?;
            let secret_key = secret
                .get("AWS_SECRET_ACCESS_KEY")
                .ok_or_else(|| anyhow!("bucket secret missing AWS_SECRET_ACCESS_KEY"))?;
            let s3 = atlas_driver_rgw::S3Target::new(
                &bucket_endpoint,
                &bucket_region,
                &bucket_name,
                access,
                secret_key,
            )?;

            // 3. (data mode) export the RBD image data and upload it to S3.
            let mut manifest: serde_json::Value =
                serde_json::from_str(&manifest_json).unwrap_or_else(|_| serde_json::json!({}));
            let mut data_summary = serde_json::Value::Null;
            let mut record_checksum: Option<String> = None;
            if mode == "data" {
                // Stream `rbd export-diff` straight into an S3 multipart upload — no in-memory cap.
                const PART_SIZE: usize = 16 * 1024 * 1024;
                let (pool_name, image) = k8s
                    .resolve_rbd(&volume_namespace, &pvc_name)
                    .await?
                    .ok_or_else(|| {
                        anyhow!("could not resolve RBD image for {volume_namespace}/{pvc_name}")
                    })?;
                let rbd_snap = format!("atlasbkp-{}", &backup_id[4..]);
                atlas_driver_ceph::rbd_snap_create(&pool_name, &image, &rbd_snap)
                    .await
                    .with_context(|| format!("rbd snap {pool_name}/{image}@{rbd_snap}"))?;
                let data_key = format!("{object_key}.rbd-diff");
                let mut child =
                    atlas_driver_ceph::rbd_export_diff_child(&pool_name, &image, &rbd_snap)
                        .map_err(|e| anyhow!("{e}"))?;
                let stdout = child
                    .stdout
                    .take()
                    .ok_or_else(|| anyhow!("rbd export-diff: no stdout"))?;
                let upload = s3
                    .put_multipart_streaming(&data_key, PART_SIZE, stdout)
                    .await;
                let (data_bytes, data_checksum) = match upload {
                    Ok(v) => v,
                    Err(e) => {
                        let _ = child.kill().await;
                        return Err(e.context(format!("stream backup data {data_key}")));
                    }
                };
                let status = child.wait().await.context("wait for rbd export-diff")?;
                if !status.success() {
                    let mut err = String::new();
                    if let Some(mut se) = child.stderr.take() {
                        use tokio::io::AsyncReadExt;
                        let _ = se.read_to_string(&mut err).await;
                    }
                    anyhow::bail!(
                        "rbd export-diff {pool_name}/{image}@{rbd_snap}: {}",
                        err.trim()
                    );
                }
                data_summary = serde_json::json!({
                    "data_object": data_key, "data_bytes": data_bytes,
                    "data_checksum": data_checksum, "format": "rbd-export-diff",
                    "rbd": format!("{pool_name}/{image}@{rbd_snap}")
                });
                manifest["data"] = data_summary.clone();
                manifest["format"] = serde_json::json!("rbd-export-diff");
                record_checksum = Some(data_checksum);
            }

            // 4. write the manifest object to RGW, then read it back to verify.
            let manifest_bytes = serde_json::to_vec(&manifest)?;
            let manifest_checksum = sha256_hex(&manifest_bytes);
            s3.put_object(&object_key, manifest_bytes.clone())
                .await
                .with_context(|| format!("PUT backup manifest {object_key}"))?;
            let verified =
                matches!(s3.get_object(&object_key).await, Ok(got) if got == manifest_bytes);
            let checksum = record_checksum.unwrap_or(manifest_checksum);
            atlas_inventory::backups::set_state(
                pool,
                &backup_id,
                if verified { "verified" } else { "completed" },
                Some(&checksum),
            )
            .await?;
            Ok(serde_json::json!({
                "backup_id": backup_id, "object_key": object_key, "mode": mode,
                "checksum": checksum, "verified": verified, "snapshot_ready": ready,
                "data": data_summary
            }))
        }
    }
}

fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    h.update(bytes);
    hex::encode(h.finalize())
}

/// Build an `S3Target` for a bucket, reading its credentials from the Rook OBC Secret in-cluster
/// (the keys are never logged or returned).
async fn build_s3_target(
    k8s: &K8sDriver,
    bucket_namespace: &str,
    bucket_secret_ref: &str,
    bucket_endpoint: &str,
    bucket_region: &str,
    bucket_name: &str,
) -> Result<atlas_driver_rgw::S3Target> {
    let secret = k8s
        .get_secret(bucket_namespace, bucket_secret_ref)
        .await?
        .ok_or_else(|| anyhow!("bucket secret {bucket_secret_ref} not found"))?;
    let access = secret
        .get("AWS_ACCESS_KEY_ID")
        .ok_or_else(|| anyhow!("bucket secret missing AWS_ACCESS_KEY_ID"))?;
    let secret_key = secret
        .get("AWS_SECRET_ACCESS_KEY")
        .ok_or_else(|| anyhow!("bucket secret missing AWS_SECRET_ACCESS_KEY"))?;
    atlas_driver_rgw::S3Target::new(
        bucket_endpoint,
        bucket_region,
        bucket_name,
        access,
        secret_key,
    )
}

/// Read a backup manifest object back from RGW (used by restore to verify integrity).
#[allow(clippy::too_many_arguments)]
async fn read_backup_manifest(
    k8s: &K8sDriver,
    bucket_namespace: &str,
    bucket_secret_ref: &str,
    bucket_endpoint: &str,
    bucket_region: &str,
    bucket_name: &str,
    object_key: &str,
) -> Result<Vec<u8>> {
    let s3 = build_s3_target(
        k8s,
        bucket_namespace,
        bucket_secret_ref,
        bucket_endpoint,
        bucket_region,
        bucket_name,
    )
    .await?;
    s3.get_object(object_key).await
}

/// Provision a new PVC populated from a VolumeSnapshot, record the volume, and protect the source
/// snapshot. Shared by clone/restore and backup-restore. Returns the PVC phase.
#[allow(clippy::too_many_arguments)]
async fn provision_from_snapshot(
    pool: &SqlitePool,
    k8s: &K8sDriver,
    tenant_id: &str,
    backend_id: &str,
    new_volume_id: &str,
    snapshot_id: &str,
    snapshot_k8s_name: &str,
    new_name: &str,
    namespace: &str,
    storage_class: &str,
    size_bytes: i64,
    access_mode: &str,
    volume_mode: &str,
    provenance: &str,
    owner: Option<OwnerRef>,
) -> Result<Option<String>> {
    let mut labels = std::collections::BTreeMap::new();
    labels.insert("zyvor.dev/volume-id".to_string(), new_volume_id.to_string());
    labels.insert(
        "zyvor.dev/from-snapshot".to_string(),
        snapshot_id.to_string(),
    );
    labels.insert("zyvor.dev/provenance".to_string(), provenance.to_string());
    if let Some(o) = &owner {
        labels.insert("zyvor.dev/owner-product".to_string(), o.product.clone());
    }
    let create = PvcCreateSpec {
        name: new_name.to_string(),
        namespace: namespace.to_string(),
        storage_class: storage_class.to_string(),
        size_bytes,
        access_modes: vec![access_mode.to_string()],
        volume_mode: Some(volume_mode.to_string()),
        labels,
        data_source_snapshot: Some(snapshot_k8s_name.to_string()),
    };
    k8s.create_pvc(&create)
        .await
        .with_context(|| format!("{provenance} PVC {namespace}/{new_name} from snapshot"))?;

    let phase = poll_pvc_phase(k8s, namespace, new_name).await;
    let vol = StorageVolume {
        id: new_volume_id.to_string(),
        cluster_id: None,
        pool_id: None,
        name: new_name.to_string(),
        kind: VolumeKind::Block,
        backend_native_id: Some(format!("pvc/{namespace}/{new_name}")),
        size_bytes,
        used_bytes: None,
        state: phase
            .clone()
            .unwrap_or_else(|| "provisioning".into())
            .to_lowercase(),
        health: if phase.as_deref() == Some("Bound") {
            Health::Ok
        } else {
            Health::Unknown
        },
        kubernetes_namespace: Some(namespace.to_string()),
        pvc_name: Some(new_name.to_string()),
        storage_class_name: Some(storage_class.to_string()),
    };
    atlas_inventory::upsert_volume(pool, backend_id, tenant_id, &vol, None).await?;
    atlas_inventory::set_volume_source_snapshot(pool, new_volume_id, snapshot_id).await?;
    atlas_inventory::snapshots::set_protected(pool, snapshot_id, true).await?;
    if let Some(o) = owner {
        atlas_inventory::insert_binding(
            pool,
            &format!("bind_{new_volume_id}"),
            tenant_id,
            &o.product,
            &o.resource_type,
            &o.resource_id,
            "volume",
            new_volume_id,
            &o.role,
        )
        .await?;
    }
    Ok(phase)
}

/// Poll for an OBC's output ConfigMap (present once the bucket is bound).
async fn poll_configmap(
    k8s: &K8sDriver,
    ns: &str,
    name: &str,
) -> Option<std::collections::BTreeMap<String, String>> {
    let deadline = std::time::Instant::now() + BIND_TIMEOUT;
    loop {
        if let Ok(Some(cm)) = k8s.get_configmap(ns, name).await {
            if cm.contains_key("BUCKET_NAME") {
                return Some(cm);
            }
        }
        if std::time::Instant::now() >= deadline {
            return None;
        }
        tokio::time::sleep(POLL_INTERVAL).await;
    }
}

fn require_k8s(k8s: &Option<Arc<K8sDriver>>) -> Result<Arc<K8sDriver>> {
    k8s.clone()
        .ok_or_else(|| anyhow!("no Kubernetes cluster is attached; cannot run the write path"))
}

async fn poll_pvc_phase(k8s: &K8sDriver, ns: &str, name: &str) -> Option<String> {
    let deadline = std::time::Instant::now() + BIND_TIMEOUT;
    loop {
        if let Ok(Some(pvc)) = k8s.get_pvc(ns, name).await {
            if pvc.phase.as_deref() == Some("Bound") {
                return Some("Bound".into());
            }
            if std::time::Instant::now() >= deadline {
                return pvc.phase;
            }
        } else if std::time::Instant::now() >= deadline {
            return None;
        }
        tokio::time::sleep(POLL_INTERVAL).await;
    }
}

async fn poll_snapshot_ready(k8s: &K8sDriver, ns: &str, name: &str) -> bool {
    let deadline = std::time::Instant::now() + BIND_TIMEOUT;
    loop {
        if let Ok(Some(true)) = k8s.volume_snapshot_ready(ns, name).await {
            return true;
        }
        if std::time::Instant::now() >= deadline {
            return false;
        }
        tokio::time::sleep(POLL_INTERVAL).await;
    }
}

fn parse_kind(s: &str) -> VolumeKind {
    match s {
        "filesystem" => VolumeKind::Filesystem,
        "object" => VolumeKind::Object,
        _ => VolumeKind::Block,
    }
}

/// Spawn the protection-schedule worker: every `tick_secs`, run any due `snapshot_schedules` by
/// enqueueing a snapshot job for their volume, advance `next_run_at`, and prune the volume's
/// scheduled snapshots to the schedule's `keep`. `tick_secs == 0` disables it.
pub fn spawn_scheduler(pool: SqlitePool, jobs: JobEngine, tick_secs: u64) {
    if tick_secs == 0 {
        tracing::info!("snapshot scheduler disabled (tick = 0)");
        return;
    }
    tokio::spawn(async move {
        tracing::info!(tick_secs, "snapshot scheduler started");
        let mut tick = tokio::time::interval(Duration::from_secs(tick_secs));
        loop {
            tick.tick().await;
            if let Err(e) = run_due_schedules(&pool, &jobs).await {
                tracing::warn!("snapshot scheduler tick failed: {e:#}");
            }
        }
    });
}

/// Marker embedded in scheduled snapshot names so retention only prunes scheduler-created snapshots.
const SCHED_MARKER: &str = "-sched-";

async fn run_due_schedules(pool: &SqlitePool, jobs: &JobEngine) -> Result<()> {
    let due = atlas_inventory::schedules::due(pool).await?;
    for sched in due {
        // Advance the clock first so a slow/failed run doesn't hot-loop this schedule.
        if let Err(e) = atlas_inventory::schedules::mark_ran(pool, &sched.id).await {
            tracing::warn!("schedule {}: mark_ran failed: {e:#}", sched.id);
            continue;
        }
        let vol = match atlas_inventory::get_volume(pool, &sched.volume_id).await {
            Ok(Some(v)) => v,
            _ => {
                tracing::warn!("schedule {}: volume {} gone", sched.id, sched.volume_id);
                continue;
            }
        };
        let (Some(namespace), Some(pvc_name)) =
            (vol.kubernetes_namespace.clone(), vol.pvc_name.clone())
        else {
            continue;
        };
        if sched.kind == "backup" {
            if let Err(e) = run_backup_schedule(pool, jobs, &sched, &namespace, &pvc_name).await {
                tracing::warn!("schedule {}: backup failed: {e:#}", sched.id);
            }
            continue;
        }
        let snapshot_id = atlas_common::ids::snapshot_id();
        let name = format!("{}{SCHED_MARKER}{}", vol.name, &snapshot_id[5..]);
        let spec = JobSpec::SnapshotCreate {
            snapshot_id,
            volume_id: sched.volume_id.clone(),
            name,
            namespace,
            pvc_name,
            snapshot_class: "zyvor-rbd-snapclass".into(),
        };
        let job_id = atlas_common::ids::job_id();
        if let Err(e) = jobs
            .enqueue(&job_id, &sched.tenant_id, "scheduler", spec, None)
            .await
        {
            tracing::warn!("schedule {}: enqueue snapshot failed: {e:#}", sched.id);
            continue;
        }
        prune_scheduled_snapshots(pool, jobs, &sched.volume_id, sched.keep).await;
    }
    Ok(())
}

/// Delete the volume's scheduler-created snapshots beyond the newest `keep` (enqueues delete jobs).
async fn prune_scheduled_snapshots(
    pool: &SqlitePool,
    jobs: &JobEngine,
    volume_id: &str,
    keep: i64,
) {
    if keep <= 0 {
        return;
    }
    let snaps = match atlas_inventory::snapshots::list_snapshots(pool, Some(volume_id)).await {
        Ok(s) => s,
        Err(_) => return,
    };
    let namespace = atlas_inventory::get_volume(pool, volume_id)
        .await
        .ok()
        .flatten()
        .and_then(|v| v.kubernetes_namespace)
        .unwrap_or_else(|| "default".into());
    // Newest-first. This runs right after enqueuing a new snapshot that isn't persisted yet, so we
    // retain `keep - 1` already-stored snapshots; together with the in-flight one the steady state
    // is exactly `keep`.
    let scheduled: Vec<_> = snaps
        .into_iter()
        .filter(|s| s.name.contains(SCHED_MARKER))
        .collect();
    let retain = (keep - 1).max(0) as usize;
    for old in scheduled.into_iter().skip(retain) {
        let spec = JobSpec::SnapshotDelete {
            snapshot_id: old.id.clone(),
            namespace: namespace.clone(),
            name: old.name,
        };
        let job_id = atlas_common::ids::job_id();
        let _ = jobs
            .enqueue(&job_id, &old.tenant_id, "scheduler", spec, None)
            .await;
    }
}

/// Run one backup schedule: snapshot + write a backup to the target bucket, then prune old backups.
async fn run_backup_schedule(
    pool: &SqlitePool,
    jobs: &JobEngine,
    sched: &atlas_api_types::SnapshotSchedule,
    namespace: &str,
    pvc_name: &str,
) -> Result<()> {
    let bucket_id = sched
        .bucket_id
        .clone()
        .ok_or_else(|| anyhow!("backup schedule {} has no bucket", sched.id))?;
    let bucket = atlas_inventory::buckets::get_bucket(pool, &bucket_id)
        .await?
        .ok_or_else(|| anyhow!("bucket {bucket_id} not found"))?;
    if bucket.state != "bound" {
        anyhow::bail!("bucket {bucket_id} not bound (state {})", bucket.state);
    }
    let bucket_endpoint = bucket
        .endpoint
        .ok_or_else(|| anyhow!("bucket has no endpoint"))?;
    let bucket_name = bucket
        .bucket_name
        .ok_or_else(|| anyhow!("bucket has no name"))?;
    let bucket_secret_ref = bucket
        .secret_ref
        .ok_or_else(|| anyhow!("bucket has no secret"))?;
    let bucket_namespace = bucket.namespace.unwrap_or_else(|| "rook-ceph".into());
    let bucket_region = bucket.region.unwrap_or_else(|| "us-east-1".into());

    let backup_id = atlas_common::ids::stable_id(
        "bkp",
        &format!("{}-{}", sched.volume_id, atlas_common::ids::job_id()),
    );
    let snapshot_id = atlas_common::ids::snapshot_id();
    let snapshot_name = format!("{pvc_name}-bkp-{}", &backup_id[4..]);
    let object_key = format!("backups/{}/{}.manifest.json", sched.volume_id, backup_id);
    let manifest = serde_json::json!({
        "backup_id": backup_id, "source_volume": sched.volume_id, "source_snapshot": snapshot_id,
        "pvc": format!("{namespace}/{pvc_name}"), "object_key": object_key,
        "format": "manifest-v1", "scheduled": sched.id,
    });
    atlas_inventory::snapshots::insert_snapshot(
        pool,
        &snapshot_id,
        &sched.tenant_id,
        &sched.volume_id,
        &snapshot_name,
        None,
        "app",
        "creating",
    )
    .await?;
    atlas_inventory::backups::insert_backup(
        pool,
        &backup_id,
        &sched.tenant_id,
        &sched.volume_id,
        Some(&snapshot_id),
        &bucket_id,
        &object_key,
        "manifest-v1",
        &manifest,
    )
    .await?;
    let spec = JobSpec::BackupCreate {
        backup_id: backup_id.clone(),
        snapshot_id,
        volume_namespace: namespace.to_string(),
        pvc_name: pvc_name.to_string(),
        snapshot_name,
        snapshot_class: "zyvor-rbd-snapclass".into(),
        object_key,
        manifest_json: manifest.to_string(),
        bucket_namespace,
        bucket_secret_ref,
        bucket_endpoint,
        bucket_name,
        bucket_region,
        mode: sched.mode.clone(),
    };
    let job_id = atlas_common::ids::job_id();
    jobs.enqueue(&job_id, &sched.tenant_id, "scheduler", spec, None)
        .await?;
    prune_scheduled_backups(pool, jobs, &sched.volume_id, sched.keep).await;
    Ok(())
}

/// Delete the volume's backups beyond the newest `keep` (enqueues backup-delete jobs). Retains
/// `keep - 1` persisted backups since one is being created this tick.
async fn prune_scheduled_backups(pool: &SqlitePool, jobs: &JobEngine, volume_id: &str, keep: i64) {
    if keep <= 0 {
        return;
    }
    let all = match atlas_inventory::backups::list_backups(pool, Some(volume_id)).await {
        Ok(a) => a,
        Err(_) => return,
    };
    let retain = (keep - 1).max(0) as usize;
    for old in all.into_iter().skip(retain) {
        let bucket = match atlas_inventory::buckets::get_bucket(pool, &old.bucket_id).await {
            Ok(Some(b)) if b.state == "bound" => b,
            _ => continue,
        };
        let vol = atlas_inventory::get_volume(pool, &old.volume_id)
            .await
            .ok()
            .flatten();
        let ns = vol
            .as_ref()
            .and_then(|v| v.kubernetes_namespace.clone())
            .unwrap_or_default();
        let pvc = vol.and_then(|v| v.pvc_name).unwrap_or_default();
        let spec = JobSpec::BackupDelete {
            backup_id: old.id.clone(),
            manifest_key: old.object_key.clone(),
            data_key: format!("{}.rbd-diff", old.object_key),
            volume_namespace: ns,
            pvc_name: pvc,
            rbd_snap: format!("atlasbkp-{}", &old.id[4..]),
            bucket_namespace: bucket.namespace.unwrap_or_else(|| "rook-ceph".into()),
            bucket_secret_ref: bucket.secret_ref.unwrap_or_default(),
            bucket_endpoint: bucket.endpoint.unwrap_or_default(),
            bucket_name: bucket.bucket_name.unwrap_or_default(),
            bucket_region: bucket.region.unwrap_or_else(|| "us-east-1".into()),
        };
        let job_id = atlas_common::ids::job_id();
        let _ = jobs
            .enqueue(&job_id, &old.tenant_id, "scheduler", spec, None)
            .await;
    }
}
