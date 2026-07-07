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
        } => {
            let k8s = require_k8s(k8s)?;
            // 1. Read the backup manifest back from RGW and verify its checksum (integrity check).
            let manifest_verified = match read_backup_manifest(
                &k8s,
                &bucket_namespace,
                &bucket_secret_ref,
                &bucket_endpoint,
                &bucket_region,
                &bucket_name,
                &object_key,
            )
            .await
            {
                Ok(bytes) => {
                    let ok = sha256_hex(&bytes) == expected_checksum;
                    if !ok {
                        tracing::warn!("backup {backup_id} manifest checksum mismatch");
                    }
                    ok
                }
                Err(e) => {
                    tracing::warn!("backup {backup_id} manifest unreadable: {e:#}");
                    false
                }
            };

            // 2. Provision a new volume from the backup's VolumeSnapshot.
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
                "volume_id": new_volume_id, "from_backup": backup_id,
                "manifest_verified": manifest_verified,
                "pvc": format!("{namespace}/{new_name}"),
                "phase": phase, "bound": phase.as_deref() == Some("Bound")
            }))
        }

        JobSpec::BucketCreate {
            bucket_id,
            namespace,
            obc_name,
            storage_class,
        } => {
            let k8s = require_k8s(k8s)?;
            k8s.create_obc(&namespace, &obc_name, &storage_class)
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
            Ok(serde_json::json!({
                "bucket_id": bucket_id, "bucket_name": bucket_name, "endpoint": endpoint
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
        } => {
            let k8s = require_k8s(k8s)?;
            // 1. point-in-time snapshot of the source volume.
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

            // 3. write the manifest object to RGW over S3, then read it back to verify.
            let s3 = atlas_driver_rgw::S3Target::new(
                &bucket_endpoint,
                &bucket_region,
                &bucket_name,
                access,
                secret_key,
            )?;
            let bytes = manifest_json.into_bytes();
            let checksum = sha256_hex(&bytes);
            s3.put_object(&object_key, bytes.clone())
                .await
                .with_context(|| format!("PUT backup manifest {object_key}"))?;
            let verified = match s3.get_object(&object_key).await {
                Ok(got) => got == bytes,
                Err(e) => {
                    tracing::warn!("backup verify GET failed: {e}");
                    false
                }
            };
            atlas_inventory::backups::set_state(
                pool,
                &backup_id,
                if verified { "verified" } else { "completed" },
                Some(&checksum),
            )
            .await?;
            Ok(serde_json::json!({
                "backup_id": backup_id, "object_key": object_key,
                "checksum": checksum, "verified": verified, "snapshot_ready": ready
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
    let s3 = atlas_driver_rgw::S3Target::new(
        bucket_endpoint,
        bucket_region,
        bucket_name,
        access,
        secret_key,
    )?;
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
