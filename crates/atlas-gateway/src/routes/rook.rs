// Copyright (c) 2026 ZyvorAI Labs Private Limited.
// SPDX-License-Identifier: AGPL-3.0-only OR LicenseRef-Atlas-Commercial
//! Rook lifecycle routes: create/list/delete `CephBlockPool`/`CephFilesystem`/`CephObjectStore`
//! CRs (+ their StorageClass) from the API instead of hand-edited YAML manifests. Mirrors
//! `routes::object_store`'s bucket create/delete shape (async job, dependent-delete guard).

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Extension, Json,
};
use serde::Deserialize;
use serde_json::{json, Value};

use atlas_common::{ids, AppError, AppResult};
use atlas_jobs::JobSpec;

use super::util::accepted;
use super::volumes::ForceParams;
use crate::auth::Actor;
use crate::state::AppState;

fn require_k8s(s: &AppState) -> AppResult<()> {
    if s.k8s.is_none() {
        return Err(AppError::Unavailable(
            "no kubernetes cluster attached; cannot manage Rook resources".into(),
        ));
    }
    Ok(())
}

// ---- pools ----

#[derive(Debug, Deserialize)]
pub(crate) struct CreatePoolBody {
    name: String,
    namespace: Option<String>,
    storage_class: Option<String>,
    replicated_size: Option<i64>,
    failure_domain: Option<String>,
    device_class: Option<String>,
}

/// `GET /ceph/pools` — every live `CephBlockPool` CR (name + phase).
pub(crate) async fn list_ceph_pools(State(s): State<AppState>) -> AppResult<Json<Value>> {
    require_k8s(&s)?;
    let pools = s
        .k8s
        .as_ref()
        .unwrap()
        .list_ceph_block_pools(&s.config.rook_namespace)
        .await
        .map_err(|e| AppError::Driver(e.to_string()))?;
    Ok(Json(json!(pools)))
}

/// `POST /ceph/pools` — create a `CephBlockPool` + matching StorageClass (async job).
pub(crate) async fn create_ceph_pool(
    State(s): State<AppState>,
    Extension(actor): Extension<Actor>,
    Json(body): Json<CreatePoolBody>,
) -> AppResult<(StatusCode, Json<Value>)> {
    crate::auth::require_role(s.config.auth_required, &actor, crate::auth::ROLE_OPERATOR)?;
    require_k8s(&s)?;
    super::util::validate_k8s_name(&body.name)?;
    let namespace = body
        .namespace
        .unwrap_or_else(|| s.config.rook_namespace.clone());
    let storage_class = body
        .storage_class
        .unwrap_or_else(|| format!("zyvor-{}", body.name));
    let spec = JobSpec::CephPoolCreate {
        name: body.name.clone(),
        namespace,
        storage_class: storage_class.clone(),
        replicated_size: body.replicated_size.unwrap_or(3).clamp(1, 9),
        failure_domain: body.failure_domain.unwrap_or_else(|| "host".into()),
        device_class: body.device_class,
    };
    let job_id = ids::job_id();
    let job = s
        .jobs
        .enqueue(&job_id, "global", &actor.id, spec, None)
        .await
        .map_err(AppError::from)?;
    let _ = atlas_inventory::audit::record(
        &s.pool,
        None,
        &actor.id,
        "ceph.pool.create.requested",
        "pool",
        &body.name,
        "accepted",
        Some(json!({ "storage_class": storage_class })),
        None,
    )
    .await;
    Ok(accepted(
        &job,
        json!({ "name": body.name, "storage_class": storage_class }),
    ))
}

/// `DELETE /ceph/pools/{name}[?force=true]` — delete the CR + StorageClass; blocked (409) while
/// any volume still references the StorageClass, unless `force=true`.
pub(crate) async fn delete_ceph_pool(
    State(s): State<AppState>,
    Extension(actor): Extension<Actor>,
    Path(name): Path<String>,
    Query(q): Query<ForceParams>,
) -> AppResult<(StatusCode, Json<Value>)> {
    crate::auth::require_role(s.config.auth_required, &actor, crate::auth::ROLE_ADMIN)?;
    require_k8s(&s)?;
    let storage_class = format!("zyvor-{name}");
    let deps = atlas_inventory::count_volumes_by_storage_class(&s.pool, &storage_class).await?;
    if deps > 0 && !q.force {
        return Err(AppError::Conflict(format!(
            "pool {name}'s StorageClass {storage_class} still has {deps} volume(s); delete them first or pass ?force=true"
        )));
    }
    let spec = JobSpec::CephPoolDelete {
        name: name.clone(),
        namespace: s.config.rook_namespace.clone(),
        storage_class,
    };
    let job_id = ids::job_id();
    let job = s
        .jobs
        .enqueue(&job_id, "global", &actor.id, spec, None)
        .await
        .map_err(AppError::from)?;
    Ok(accepted(&job, json!({ "name": name })))
}

// ---- filesystems ----

#[derive(Debug, Deserialize)]
pub(crate) struct CreateFilesystemBody {
    name: String,
    namespace: Option<String>,
    storage_class: Option<String>,
    data_pool_name: Option<String>,
    replicated_size: Option<i64>,
}

/// `GET /ceph/filesystems` — every live `CephFilesystem` CR (name + phase).
pub(crate) async fn list_ceph_filesystems(State(s): State<AppState>) -> AppResult<Json<Value>> {
    require_k8s(&s)?;
    let fs = s
        .k8s
        .as_ref()
        .unwrap()
        .list_ceph_filesystems(&s.config.rook_namespace)
        .await
        .map_err(|e| AppError::Driver(e.to_string()))?;
    Ok(Json(json!(fs)))
}

/// `POST /ceph/filesystems` — create a `CephFilesystem` (RWX CephFS) + StorageClass (async job).
pub(crate) async fn create_ceph_filesystem(
    State(s): State<AppState>,
    Extension(actor): Extension<Actor>,
    Json(body): Json<CreateFilesystemBody>,
) -> AppResult<(StatusCode, Json<Value>)> {
    crate::auth::require_role(s.config.auth_required, &actor, crate::auth::ROLE_OPERATOR)?;
    require_k8s(&s)?;
    super::util::validate_k8s_name(&body.name)?;
    let namespace = body
        .namespace
        .unwrap_or_else(|| s.config.rook_namespace.clone());
    let storage_class = body
        .storage_class
        .unwrap_or_else(|| format!("zyvor-{}-shared", body.name));
    let spec = JobSpec::CephFilesystemCreate {
        name: body.name.clone(),
        namespace,
        storage_class: storage_class.clone(),
        data_pool_name: body.data_pool_name.unwrap_or_else(|| "data0".into()),
        replicated_size: body.replicated_size.unwrap_or(3).clamp(1, 9),
    };
    let job_id = ids::job_id();
    let job = s
        .jobs
        .enqueue(&job_id, "global", &actor.id, spec, None)
        .await
        .map_err(AppError::from)?;
    Ok(accepted(
        &job,
        json!({ "name": body.name, "storage_class": storage_class }),
    ))
}

/// `DELETE /ceph/filesystems/{name}[?force=true]`.
pub(crate) async fn delete_ceph_filesystem(
    State(s): State<AppState>,
    Extension(actor): Extension<Actor>,
    Path(name): Path<String>,
    Query(q): Query<ForceParams>,
) -> AppResult<(StatusCode, Json<Value>)> {
    crate::auth::require_role(s.config.auth_required, &actor, crate::auth::ROLE_ADMIN)?;
    require_k8s(&s)?;
    let storage_class = format!("zyvor-{name}-shared");
    let deps = atlas_inventory::count_volumes_by_storage_class(&s.pool, &storage_class).await?;
    if deps > 0 && !q.force {
        return Err(AppError::Conflict(format!(
            "filesystem {name}'s StorageClass {storage_class} still has {deps} volume(s); delete them first or pass ?force=true"
        )));
    }
    let spec = JobSpec::CephFilesystemDelete {
        name: name.clone(),
        namespace: s.config.rook_namespace.clone(),
        storage_class,
    };
    let job_id = ids::job_id();
    let job = s
        .jobs
        .enqueue(&job_id, "global", &actor.id, spec, None)
        .await
        .map_err(AppError::from)?;
    Ok(accepted(&job, json!({ "name": name })))
}

// ---- object stores ----

#[derive(Debug, Deserialize)]
pub(crate) struct CreateObjectStoreBody {
    name: String,
    namespace: Option<String>,
    storage_class: Option<String>,
    replicated_size: Option<i64>,
    gateway_port: Option<i64>,
    gateway_instances: Option<i64>,
}

/// `GET /ceph/object-stores` — every live `CephObjectStore` CR (name + phase).
pub(crate) async fn list_ceph_object_stores(State(s): State<AppState>) -> AppResult<Json<Value>> {
    require_k8s(&s)?;
    let stores = s
        .k8s
        .as_ref()
        .unwrap()
        .list_ceph_object_stores(&s.config.rook_namespace)
        .await
        .map_err(|e| AppError::Driver(e.to_string()))?;
    Ok(Json(json!(stores)))
}

/// `POST /ceph/object-stores` — create a `CephObjectStore` (RGW) + bucket StorageClass (async job).
pub(crate) async fn create_ceph_object_store(
    State(s): State<AppState>,
    Extension(actor): Extension<Actor>,
    Json(body): Json<CreateObjectStoreBody>,
) -> AppResult<(StatusCode, Json<Value>)> {
    crate::auth::require_role(s.config.auth_required, &actor, crate::auth::ROLE_OPERATOR)?;
    require_k8s(&s)?;
    super::util::validate_k8s_name(&body.name)?;
    let namespace = body
        .namespace
        .unwrap_or_else(|| s.config.rook_namespace.clone());
    let storage_class = body
        .storage_class
        .unwrap_or_else(|| format!("zyvor-{}-bucket", body.name));
    let spec = JobSpec::CephObjectStoreCreate {
        name: body.name.clone(),
        namespace,
        storage_class: storage_class.clone(),
        replicated_size: body.replicated_size.unwrap_or(3).clamp(1, 9),
        gateway_port: body.gateway_port.unwrap_or(80),
        gateway_instances: body.gateway_instances.unwrap_or(1).max(1),
    };
    let job_id = ids::job_id();
    let job = s
        .jobs
        .enqueue(&job_id, "global", &actor.id, spec, None)
        .await
        .map_err(AppError::from)?;
    Ok(accepted(
        &job,
        json!({ "name": body.name, "storage_class": storage_class }),
    ))
}

/// `DELETE /ceph/object-stores/{name}[?force=true]` — blocked (409) while any bucket still
/// references the store's StorageClass.
pub(crate) async fn delete_ceph_object_store(
    State(s): State<AppState>,
    Extension(actor): Extension<Actor>,
    Path(name): Path<String>,
    Query(q): Query<ForceParams>,
) -> AppResult<(StatusCode, Json<Value>)> {
    crate::auth::require_role(s.config.auth_required, &actor, crate::auth::ROLE_ADMIN)?;
    require_k8s(&s)?;
    let storage_class = format!("zyvor-{name}-bucket");
    let deps = atlas_inventory::buckets::count_by_storage_class(&s.pool, &storage_class).await?;
    if deps > 0 && !q.force {
        return Err(AppError::Conflict(format!(
            "object store {name}'s StorageClass {storage_class} still has {deps} bucket(s); delete them first or pass ?force=true"
        )));
    }
    let spec = JobSpec::CephObjectStoreDelete {
        name: name.clone(),
        namespace: s.config.rook_namespace.clone(),
        storage_class,
    };
    let job_id = ids::job_id();
    let job = s
        .jobs
        .enqueue(&job_id, "global", &actor.id, spec, None)
        .await
        .map_err(AppError::from)?;
    Ok(accepted(&job, json!({ "name": name })))
}
