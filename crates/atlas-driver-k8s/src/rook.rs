// Copyright (c) 2026 ZyvorAI Labs Private Limited.
// SPDX-License-Identifier: AGPL-3.0-only OR LicenseRef-Atlas-Commercial
//! Rook (`ceph.rook.io/v1`) CR helpers built on the generic `apply_cr`/`get_cr_status`/`list_crs`
//! mechanism in `lib.rs`. Returns raw `serde_json::Value`/plain structs like the rest of that
//! generic layer — no typed CRD codegen dependency, since Rook's CR schemas are wide and this
//! only needs a handful of fields (`status.phase`, `status.ceph.health`).
//!
//! This is a second, precise source of truth alongside `atlas-driver-ceph`'s `ceph`/`rbd` CLI
//! path — additive, not a replacement: the CLI path still works where the k8s driver isn't
//! available at all.

use crate::{K8sDriver, K8sError};
use kube::core::DynamicObject;
use std::collections::HashMap;

pub const ROOK_GROUP: &str = "ceph.rook.io";
pub const ROOK_VERSION: &str = "v1";

fn phase_of(obj: &DynamicObject) -> Option<String> {
    obj.data
        .get("status")?
        .get("phase")?
        .as_str()
        .map(|s| s.to_string())
}

fn name_of(obj: &DynamicObject) -> String {
    obj.metadata.name.clone().unwrap_or_default()
}

/// Name + `status.phase` of a Rook CR — the compact view list endpoints need.
#[derive(Debug, Clone, serde::Serialize)]
pub struct RookCrStatus {
    pub name: String,
    pub phase: Option<String>,
}

/// Precise pool classification derived from live Rook CRs (see `classify_pool_via_rook`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RookPoolKind {
    Rbd,
    CephfsData,
    CephfsMetadata,
    Rgw,
}

impl RookPoolKind {
    /// The exact string values `atlas-driver-ceph::real::pool_kind_from_name` already produces,
    /// so callers can drop this straight into `StoragePool::kind` without a translation layer.
    pub fn as_str(&self) -> &'static str {
        match self {
            RookPoolKind::Rbd => "rbd",
            RookPoolKind::CephfsData => "cephfs_data",
            RookPoolKind::CephfsMetadata => "cephfs_metadata",
            RookPoolKind::Rgw => "rgw",
        }
    }
}

impl K8sDriver {
    /// `CephCluster.status` (`.ceph.health`, `.phase`, `.storage.deviceClasses`, ...).
    pub async fn get_ceph_cluster_status(
        &self,
        ns: &str,
        name: &str,
    ) -> Result<Option<serde_json::Value>, K8sError> {
        self.get_cr_status(ROOK_GROUP, ROOK_VERSION, "CephCluster", ns, name)
            .await
    }

    /// Every `CephBlockPool` CR in `ns`, with its `status.phase`.
    pub async fn list_ceph_block_pools(&self, ns: &str) -> Result<Vec<RookCrStatus>, K8sError> {
        let items = self
            .list_crs(ROOK_GROUP, ROOK_VERSION, "CephBlockPool", ns)
            .await?;
        Ok(items
            .iter()
            .map(|o| RookCrStatus {
                name: name_of(o),
                phase: phase_of(o),
            })
            .collect())
    }

    /// Every `CephFilesystem` CR in `ns`, with its `status.phase`.
    pub async fn list_ceph_filesystems(&self, ns: &str) -> Result<Vec<RookCrStatus>, K8sError> {
        let items = self
            .list_crs(ROOK_GROUP, ROOK_VERSION, "CephFilesystem", ns)
            .await?;
        Ok(items
            .iter()
            .map(|o| RookCrStatus {
                name: name_of(o),
                phase: phase_of(o),
            })
            .collect())
    }

    /// Every `CephObjectStore` CR in `ns`, with its `status.phase`.
    pub async fn list_ceph_object_stores(&self, ns: &str) -> Result<Vec<RookCrStatus>, K8sError> {
        let items = self
            .list_crs(ROOK_GROUP, ROOK_VERSION, "CephObjectStore", ns)
            .await?;
        Ok(items
            .iter()
            .map(|o| RookCrStatus {
                name: name_of(o),
                phase: phase_of(o),
            })
            .collect())
    }

    /// Build the exact set of pool names Rook itself will have created, mapped to their precise
    /// kind — an ahead-of-time replacement for the name-heuristic fallback in
    /// `atlas-driver-ceph::real::pool_kind_from_name`. Mirrors the `RbdOwners` pattern
    /// (`rbd_image_owners`): built once per discovery pass from cluster CRs and handed to the
    /// discovery enrichment step, so `atlas-discovery` stays decoupled from the Kubernetes driver.
    ///
    /// Naming convention (Rook v1.20): a `CephBlockPool` CR's `metadata.name` *is* the pool name;
    /// a `CephFilesystem` named `fs` creates `fs-metadata` (metadata pool) and `fs-<dataPool.name>`
    /// (each entry in `spec.dataPools`); a `CephObjectStore` named `store` creates `store.rgw.*`
    /// pools (control/meta/log/buckets.index/buckets.data/buckets.non-ec/otp).
    pub async fn known_rook_pool_kinds(
        &self,
        ns: &str,
    ) -> Result<HashMap<String, RookPoolKind>, K8sError> {
        let mut out = HashMap::new();

        for p in self
            .list_crs(ROOK_GROUP, ROOK_VERSION, "CephBlockPool", ns)
            .await?
        {
            out.insert(name_of(&p), RookPoolKind::Rbd);
        }

        for fs in self
            .list_crs(ROOK_GROUP, ROOK_VERSION, "CephFilesystem", ns)
            .await?
        {
            let fs_name = name_of(&fs);
            out.insert(format!("{fs_name}-metadata"), RookPoolKind::CephfsMetadata);
            if let Some(data_pools) = fs.data.pointer("/spec/dataPools").and_then(|v| v.as_array())
            {
                for dp in data_pools {
                    if let Some(dp_name) = dp.get("name").and_then(|v| v.as_str()) {
                        out.insert(format!("{fs_name}-{dp_name}"), RookPoolKind::CephfsData);
                    }
                }
            }
        }

        for os in self
            .list_crs(ROOK_GROUP, ROOK_VERSION, "CephObjectStore", ns)
            .await?
        {
            let os_name = name_of(&os);
            for suffix in [
                "control",
                "meta",
                "log",
                "buckets.index",
                "buckets.data",
                "buckets.non-ec",
                "otp",
            ] {
                out.insert(format!("{os_name}.rgw.{suffix}"), RookPoolKind::Rgw);
            }
        }

        Ok(out)
    }

    /// Classify a single pool name via `known_rook_pool_kinds`. `None` means no Rook CR claims
    /// this pool — the caller should fall back to the old name heuristic (e.g. non-Rook Ceph).
    pub async fn classify_pool_via_rook(
        &self,
        ns: &str,
        pool_name: &str,
    ) -> Result<Option<RookPoolKind>, K8sError> {
        Ok(self.known_rook_pool_kinds(ns).await?.get(pool_name).copied())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn obj_with_status(name: &str, phase: &str) -> DynamicObject {
        let mut obj = DynamicObject::new(
            name,
            &kube::core::ApiResource::from_gvk(&kube::core::GroupVersionKind::gvk(
                ROOK_GROUP,
                ROOK_VERSION,
                "CephBlockPool",
            )),
        );
        obj.data = serde_json::json!({ "status": { "phase": phase } });
        obj
    }

    #[test]
    fn phase_of_reads_status_phase() {
        let obj = obj_with_status("rbd-nvme-prod", "Ready");
        assert_eq!(phase_of(&obj), Some("Ready".to_string()));
    }

    #[test]
    fn phase_of_none_when_no_status() {
        let obj = DynamicObject::new(
            "x",
            &kube::core::ApiResource::from_gvk(&kube::core::GroupVersionKind::gvk(
                ROOK_GROUP,
                ROOK_VERSION,
                "CephBlockPool",
            )),
        );
        assert_eq!(phase_of(&obj), None);
    }
}
