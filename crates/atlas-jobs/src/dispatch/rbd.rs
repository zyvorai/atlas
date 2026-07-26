// Copyright (c) 2026 ZyvorAI Labs Private Limited. All rights reserved.
use anyhow::{Context, Result};
use atlas_api_types::{Health, StorageVolume, VolumeKind};
use sqlx::SqlitePool;
use std::sync::Arc;
use atlas_driver_k8s::K8sDriver;

use crate::spec::JobSpec;

pub(crate) async fn dispatch_rbd(
    pool: &SqlitePool,
    _k8s: &Option<Arc<K8sDriver>>,
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
        JobSpec::RbdResize {
            volume_id,
            pool: rbd_pool,
            image,
            new_size_bytes,
            allow_shrink,
        } => {
            atlas_driver_ceph::rbd_resize(&rbd_pool, &image, new_size_bytes, allow_shrink)
                .await
                .with_context(|| format!("rbd resize {rbd_pool}/{image}"))?;
            atlas_inventory::set_volume_size(pool, &volume_id, new_size_bytes).await?;
            Ok(serde_json::json!({
                "volume_id": volume_id, "rbd": format!("{rbd_pool}/{image}"),
                "new_size_bytes": new_size_bytes
            }))
        }
        JobSpec::RbdMigrate { volume_id, pool: rbd_pool, image, dest_pool } => {
            atlas_driver_ceph::rbd_migrate(&rbd_pool, &image, &dest_pool)
                .await
                .with_context(|| format!("rbd migrate {rbd_pool}/{image} -> {dest_pool}"))?;
            Ok(serde_json::json!({
                "volume_id": volume_id, "from": format!("{rbd_pool}/{image}"),
                "to": format!("{dest_pool}/{image}")
            }))
        }
        JobSpec::RbdFlatten {
            pool: rbd_pool,
            image,
        } => {
            atlas_driver_ceph::rbd_flatten(&rbd_pool, &image)
                .await
                .with_context(|| format!("rbd flatten {rbd_pool}/{image}"))?;
            Ok(serde_json::json!({ "rbd": format!("{rbd_pool}/{image}"), "flattened": true }))
        }
        JobSpec::RbdQos { volume_id, pool: rbd_pool, image, iops_limit, bps_limit } => {
            atlas_driver_ceph::rbd_qos_set(&rbd_pool, &image, iops_limit, bps_limit)
                .await
                .with_context(|| format!("rbd qos {rbd_pool}/{image}"))?;
            if !volume_id.is_empty() {
                atlas_inventory::set_volume_qos(pool, &volume_id, iops_limit, bps_limit).await?;
            }
            Ok(serde_json::json!({
                "rbd": format!("{rbd_pool}/{image}"),
                "iops_limit": iops_limit, "bps_limit": bps_limit
            }))
        }
        JobSpec::CephOsdOp { osd_id, action, weight } => {
            atlas_driver_ceph::ceph_osd_op(&action, osd_id, weight)
                .await
                .with_context(|| format!("ceph osd {action} {osd_id}"))?;
            Ok(serde_json::json!({ "osd_id": osd_id, "action": action, "weight": weight, "applied": true }))
        }
        JobSpec::RbdMirror {
            mirror_id,
            pool: rbd_pool,
            image,
            action,
            mode,
            force,
        } => {
            // Fake driver: catalog transitions are applied by the API before enqueue; skip the live
            // `rbd` CLI and do not rewrite role/state here (avoids racing a later demote/promote).
            let fake = !matches!(
                std::env::var("ATLAS_CEPH_DRIVER_MODE")
                    .unwrap_or_default()
                    .to_ascii_lowercase()
                    .as_str(),
                "real"
            );
            if fake {
                tracing::info!(
                    %action,
                    force,
                    "fake mode: skipping rbd mirror {rbd_pool}/{image}"
                );
            } else if let Err(e) =
                atlas_driver_ceph::rbd_mirror_op(&action, &rbd_pool, &image, &mode, force).await
            {
                let err = format!("{e:#}");
                let _ = atlas_inventory::dr::set_mirror_error(pool, &mirror_id, &err).await;
                return Err(e).with_context(|| format!("rbd mirror {action} {rbd_pool}/{image}"));
            } else {
                // Real mode: reflect the resulting role/state after the CLI succeeds.
                // `enable` only clears pending state — role was set by the API at insert time.
                match action.as_str() {
                    "promote" => {
                        atlas_inventory::dr::set_mirror(pool, &mirror_id, "primary", "enabled")
                            .await?;
                        let _ = atlas_inventory::dr::record_failover(pool, &mirror_id, force).await;
                    }
                    "demote" => {
                        atlas_inventory::dr::set_mirror(pool, &mirror_id, "secondary", "enabled")
                            .await?;
                    }
                    "disable" => {
                        atlas_inventory::dr::set_mirror(pool, &mirror_id, "primary", "disabled")
                            .await?;
                    }
                    "enable" => {
                        atlas_inventory::dr::set_mirror_state(pool, &mirror_id, "enabled").await?;
                    }
                    _ => {}
                }
            }
            Ok(serde_json::json!({
                "mirror_id": mirror_id, "rbd": format!("{rbd_pool}/{image}"),
                "action": action, "force": force, "fake": fake
            }))
        }
        JobSpec::RbdSnapshot {
            pool: rbd_pool,
            image,
            snap,
        } => {
            atlas_driver_ceph::rbd_snap_create(&rbd_pool, &image, &snap)
                .await
                .with_context(|| format!("rbd snap create {rbd_pool}/{image}@{snap}"))?;
            Ok(serde_json::json!({ "snapshot": format!("{rbd_pool}/{image}@{snap}") }))
        }
        JobSpec::RbdRollback {
            pool: rbd_pool,
            image,
            snap,
        } => {
            atlas_driver_ceph::rbd_snap_rollback(&rbd_pool, &image, &snap)
                .await
                .with_context(|| format!("rbd snap rollback {rbd_pool}/{image}@{snap}"))?;
            Ok(serde_json::json!({
                "rbd": format!("{rbd_pool}/{image}"), "rolled_back_to": snap
            }))
        }
        _ => anyhow::bail!("not an rbd spec"),
    }
}
