// Copyright (c) 2026 ZyvorAI Labs Private Limited.
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-1.0
//! Rook lifecycle jobs: create/delete a `CephBlockPool`/`CephFilesystem`/`CephObjectStore` CR plus
//! its matching StorageClass, via the generic `apply_cr`/`delete_cr`/`apply_storage_class` k8s
//! mechanism — the same one CloudNativePG/Percona/Strimzi CRs already go through
//! (`atlas_driver_k8s::K8sDriver`). CR shapes mirror the static manifests in
//! `deploy/rook-ceph-lab/{03-rbd-blockpool,04-cephfs,05-rgw-objectstore}*.yaml`.

use std::collections::BTreeMap;
use std::sync::Arc;

use anyhow::Result;
use atlas_driver_k8s::K8sDriver;
use sqlx::AnyPool;

use super::helpers::{poll_cr_ready, require_k8s};
use crate::spec::JobSpec;

const ROOK_GROUP: &str = "ceph.rook.io";
const ROOK_VERSION: &str = "v1";

fn csi_secret_params(prefix: &str, ns: &str) -> Vec<(String, String)> {
    [
        "provisioner-secret-name",
        "provisioner-secret-namespace",
        "controller-expand-secret-name",
        "controller-expand-secret-namespace",
        "node-stage-secret-name",
        "node-stage-secret-namespace",
    ]
    .iter()
    .map(|suffix| {
        let key = format!("csi.storage.k8s.io/{suffix}");
        let value = if suffix.ends_with("namespace") {
            ns.to_string()
        } else {
            format!(
                "rook-csi-{prefix}-{}",
                if suffix.starts_with("node") {
                    "node"
                } else {
                    "provisioner"
                }
            )
        };
        (key, value)
    })
    .collect()
}

pub(crate) async fn dispatch_rook(
    _pool: &AnyPool,
    k8s: &Option<Arc<K8sDriver>>,
    _tenant_id: &str,
    spec: JobSpec,
) -> Result<serde_json::Value> {
    match spec {
        JobSpec::CephPoolCreate {
            name,
            namespace,
            storage_class,
            replicated_size,
            failure_domain,
            device_class,
        } => {
            let k8s = require_k8s(k8s)?;
            let mut cr_spec = serde_json::json!({
                "failureDomain": failure_domain,
                "replicated": { "size": replicated_size, "requireSafeReplicaSize": false },
            });
            if let Some(dc) = &device_class {
                cr_spec["deviceClass"] = serde_json::json!(dc);
            }
            k8s.apply_cr(
                ROOK_GROUP,
                ROOK_VERSION,
                "CephBlockPool",
                &namespace,
                &name,
                cr_spec,
            )
            .await?;

            let mut params: BTreeMap<String, String> = BTreeMap::new();
            params.insert("clusterID".into(), namespace.clone());
            params.insert("pool".into(), name.clone());
            params.insert("imageFormat".into(), "2".into());
            params.insert(
                "imageFeatures".into(),
                "layering,fast-diff,object-map,deep-flatten,exclusive-lock".into(),
            );
            params.insert("csi.storage.k8s.io/fstype".into(), "ext4".into());
            for (k, v) in csi_secret_params("rbd", &namespace) {
                params.insert(k, v);
            }
            let mut labels = BTreeMap::new();
            labels.insert("zyvor.dev/storage-backend".into(), "ceph".into());
            labels.insert("zyvor.dev/storage-kind".into(), "block".into());
            k8s.apply_storage_class(
                &storage_class,
                "rook-ceph.rbd.csi.ceph.com",
                &params,
                &labels,
                true,
            )
            .await?;

            let phase = poll_cr_ready(
                &k8s,
                ROOK_GROUP,
                ROOK_VERSION,
                "CephBlockPool",
                &namespace,
                &name,
            )
            .await;
            Ok(serde_json::json!({
                "pool": name, "storage_class": storage_class, "phase": phase,
                "ready": phase.as_deref() == Some("Ready")
            }))
        }

        JobSpec::CephPoolDelete {
            name,
            namespace,
            storage_class,
        } => {
            let k8s = require_k8s(k8s)?;
            k8s.delete_storage_class(&storage_class).await?;
            k8s.delete_cr(ROOK_GROUP, ROOK_VERSION, "CephBlockPool", &namespace, &name)
                .await?;
            Ok(serde_json::json!({ "pool": name, "deleted": true }))
        }

        JobSpec::CephFilesystemCreate {
            name,
            namespace,
            storage_class,
            data_pool_name,
            replicated_size,
        } => {
            let k8s = require_k8s(k8s)?;
            let replicated =
                serde_json::json!({ "size": replicated_size, "requireSafeReplicaSize": false });
            let cr_spec = serde_json::json!({
                "metadataPool": { "replicated": replicated.clone() },
                "dataPools": [{ "name": data_pool_name, "replicated": replicated }],
                "metadataServer": { "activeCount": 1, "activeStandby": true },
            });
            k8s.apply_cr(
                ROOK_GROUP,
                ROOK_VERSION,
                "CephFilesystem",
                &namespace,
                &name,
                cr_spec,
            )
            .await?;

            let mut params: BTreeMap<String, String> = BTreeMap::new();
            params.insert("clusterID".into(), namespace.clone());
            params.insert("fsName".into(), name.clone());
            params.insert("pool".into(), format!("{name}-{data_pool_name}"));
            for (k, v) in csi_secret_params("cephfs", &namespace) {
                params.insert(k, v);
            }
            let mut labels = BTreeMap::new();
            labels.insert("zyvor.dev/storage-backend".into(), "ceph".into());
            labels.insert("zyvor.dev/storage-kind".into(), "file".into());
            labels.insert("zyvor.dev/access-mode".into(), "rwx".into());
            k8s.apply_storage_class(
                &storage_class,
                "rook-ceph.cephfs.csi.ceph.com",
                &params,
                &labels,
                true,
            )
            .await?;

            let phase = poll_cr_ready(
                &k8s,
                ROOK_GROUP,
                ROOK_VERSION,
                "CephFilesystem",
                &namespace,
                &name,
            )
            .await;
            Ok(serde_json::json!({
                "filesystem": name, "storage_class": storage_class, "phase": phase,
                "ready": phase.as_deref() == Some("Ready")
            }))
        }

        JobSpec::CephFilesystemDelete {
            name,
            namespace,
            storage_class,
        } => {
            let k8s = require_k8s(k8s)?;
            k8s.delete_storage_class(&storage_class).await?;
            k8s.delete_cr(
                ROOK_GROUP,
                ROOK_VERSION,
                "CephFilesystem",
                &namespace,
                &name,
            )
            .await?;
            Ok(serde_json::json!({ "filesystem": name, "deleted": true }))
        }

        JobSpec::CephObjectStoreCreate {
            name,
            namespace,
            storage_class,
            replicated_size,
            gateway_port,
            gateway_instances,
        } => {
            let k8s = require_k8s(k8s)?;
            let replicated =
                serde_json::json!({ "size": replicated_size, "requireSafeReplicaSize": false });
            let cr_spec = serde_json::json!({
                "metadataPool": { "failureDomain": "host", "replicated": replicated.clone() },
                "dataPool": { "failureDomain": "host", "replicated": replicated },
                "preservePoolsOnDelete": false,
                "gateway": { "port": gateway_port, "instances": gateway_instances },
            });
            k8s.apply_cr(
                ROOK_GROUP,
                ROOK_VERSION,
                "CephObjectStore",
                &namespace,
                &name,
                cr_spec,
            )
            .await?;

            let mut params: BTreeMap<String, String> = BTreeMap::new();
            params.insert("objectStoreName".into(), name.clone());
            params.insert("objectStoreNamespace".into(), namespace.clone());
            let mut labels = BTreeMap::new();
            labels.insert("zyvor.dev/storage-backend".into(), "ceph".into());
            labels.insert("zyvor.dev/storage-kind".into(), "object".into());
            k8s.apply_storage_class(
                &storage_class,
                "rook-ceph.ceph.rook.io/bucket",
                &params,
                &labels,
                false,
            )
            .await?;

            let phase = poll_cr_ready(
                &k8s,
                ROOK_GROUP,
                ROOK_VERSION,
                "CephObjectStore",
                &namespace,
                &name,
            )
            .await;
            Ok(serde_json::json!({
                "object_store": name, "storage_class": storage_class, "phase": phase,
                "ready": phase.as_deref() == Some("Ready")
            }))
        }

        JobSpec::CephObjectStoreDelete {
            name,
            namespace,
            storage_class,
        } => {
            let k8s = require_k8s(k8s)?;
            k8s.delete_storage_class(&storage_class).await?;
            k8s.delete_cr(
                ROOK_GROUP,
                ROOK_VERSION,
                "CephObjectStore",
                &namespace,
                &name,
            )
            .await?;
            Ok(serde_json::json!({ "object_store": name, "deleted": true }))
        }

        _ => anyhow::bail!("not a rook spec"),
    }
}
