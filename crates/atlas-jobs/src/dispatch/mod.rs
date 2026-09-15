// Copyright (c) 2026 ZyvorAI Labs Private Limited.
// SPDX-License-Identifier: AGPL-3.0-only OR LicenseRef-Atlas-Commercial
mod databridge;
mod helpers;
mod object;
mod rbd;
mod rook;
mod volumes;

use std::sync::Arc;

use anyhow::Result;
use atlas_driver_k8s::K8sDriver;
use sqlx::AnyPool;

use crate::spec::JobSpec;

#[tracing::instrument(skip(pool, k8s, spec), fields(tenant_id = %tenant_id))]
pub(crate) async fn dispatch(
    pool: &AnyPool,
    k8s: &Option<Arc<K8sDriver>>,
    tenant_id: &str,
    spec: JobSpec,
) -> Result<serde_json::Value> {
    match &spec {
        JobSpec::RbdCreate { .. }
        | JobSpec::RbdDelete { .. }
        | JobSpec::RbdClone { .. }
        | JobSpec::RbdResize { .. }
        | JobSpec::RbdMigrate { .. }
        | JobSpec::RbdFlatten { .. }
        | JobSpec::RbdQos { .. }
        | JobSpec::CephOsdOp { .. }
        | JobSpec::RbdMirror { .. }
        | JobSpec::RbdSnapshot { .. }
        | JobSpec::RbdRollback { .. }
        | JobSpec::RbdSnapDelete { .. } => rbd::dispatch_rbd(pool, k8s, tenant_id, spec).await,
        JobSpec::VolumeCreate { .. }
        | JobSpec::VolumeDelete { .. }
        | JobSpec::VolumeExpand { .. }
        | JobSpec::SnapshotCreate { .. }
        | JobSpec::SnapshotDelete { .. }
        | JobSpec::SnapshotClone { .. } => {
            volumes::dispatch_volumes(pool, k8s, tenant_id, spec).await
        }
        JobSpec::BucketCreate { .. }
        | JobSpec::BackupCreate { .. }
        | JobSpec::RestoreBackup { .. }
        | JobSpec::BackupDelete { .. }
        | JobSpec::BucketDelete { .. } => object::dispatch_object(pool, k8s, tenant_id, spec).await,
        JobSpec::CephPoolCreate { .. }
        | JobSpec::CephPoolDelete { .. }
        | JobSpec::CephFilesystemCreate { .. }
        | JobSpec::CephFilesystemDelete { .. }
        | JobSpec::CephObjectStoreCreate { .. }
        | JobSpec::CephObjectStoreDelete { .. } => {
            rook::dispatch_rook(pool, k8s, tenant_id, spec).await
        }
        JobSpec::SourceDiscover { .. }
        | JobSpec::MigrationAssess { .. }
        | JobSpec::EdgeDbProvision { .. }
        | JobSpec::FullLoad { .. }
        | JobSpec::CdcStart { .. }
        | JobSpec::CdcStop { .. }
        | JobSpec::CdcRestart { .. }
        | JobSpec::ValidateRun { .. }
        | JobSpec::Cutover { .. }
        | JobSpec::Rollback { .. }
        | JobSpec::ObjectMigrate { .. } => databridge::dispatch_databridge(pool, k8s, spec).await,
    }
}
