// Copyright (c) 2026 ZyvorAI Labs Private Limited. All rights reserved.
use std::sync::Arc;
use std::time::Duration;

use anyhow::{anyhow, Context, Result};
use atlas_api_types::{Health, StorageVolume, VolumeKind};
use atlas_driver_k8s::{K8sDriver, PvcCreateSpec};
use sqlx::SqlitePool;

use crate::spec::OwnerRef;

pub(crate) const BIND_TIMEOUT: Duration = Duration::from_secs(45);
pub(crate) const POLL_INTERVAL: Duration = Duration::from_secs(2);

pub(crate) fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    h.update(bytes);
    hex::encode(h.finalize())
}

/// Build an `S3Target` for a bucket, reading its credentials from the Rook OBC Secret in-cluster
/// (the keys are never logged or returned).
pub(crate) async fn build_s3_target(
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
pub(crate) async fn read_backup_manifest(
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
pub(crate) async fn provision_from_snapshot(
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
pub(crate) async fn poll_configmap(
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

pub(crate) fn require_k8s(k8s: &Option<Arc<K8sDriver>>) -> Result<Arc<K8sDriver>> {
    k8s.clone()
        .ok_or_else(|| anyhow!("no Kubernetes cluster is attached; cannot run the write path"))
}

pub(crate) async fn poll_pvc_phase(k8s: &K8sDriver, ns: &str, name: &str) -> Option<String> {
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

pub(crate) async fn poll_snapshot_ready(k8s: &K8sDriver, ns: &str, name: &str) -> bool {
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

pub(crate) fn parse_kind(s: &str) -> VolumeKind {
    match s {
        "filesystem" => VolumeKind::Filesystem,
        "object" => VolumeKind::Object,
        _ => VolumeKind::Block,
    }
}
