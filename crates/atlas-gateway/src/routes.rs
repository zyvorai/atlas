// Copyright (c) 2026 ZyvorAI Labs Private Limited. All rights reserved.
//! REST surface. Slice 1 read-only inventory/discovery (PDF §10.2) + slice 2 async write path
//! (volume create/expand/delete, snapshots) — write ops return `202 Accepted` with a job id.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    middleware,
    routing::{get, post},
    Extension, Json, Router,
};
use serde::Deserialize;
use serde_json::{json, Value};

use atlas_api_types::{
    BackendMode, BackendType, Capabilities, CreateVolumeRequest, StorageBackend, StorageClassInfo,
};
use atlas_common::{ids, AppError, AppResult};
use atlas_jobs::{JobSpec, OwnerRef};

use crate::auth::{auth_middleware, Actor};
use crate::state::AppState;

pub fn router(state: AppState) -> Router {
    let api = Router::new()
        .route("/backends", get(list_backends).post(create_backend))
        .route("/backends/{id}/discover", post(discover_backend))
        .route("/clusters", get(list_clusters))
        .route("/clusters/{id}/health", get(cluster_health))
        .route("/clusters/{id}/capabilities", get(cluster_capabilities))
        .route("/nodes", get(list_nodes))
        .route("/osds", get(list_osds))
        .route("/pools", get(list_pools))
        .route("/storage-classes", get(list_storage_classes))
        .route("/kubernetes/pvcs", get(list_pvcs))
        .route("/kubernetes/pvs", get(list_pvs))
        .route("/volumes", get(list_volumes).post(create_volume))
        .route("/volumes/{id}", get(get_volume).delete(delete_volume))
        .route("/volumes/{id}/expand", post(expand_volume))
        .route("/volumes/{id}/snapshots", post(create_snapshot))
        .route("/snapshots", get(list_snapshots))
        .route("/snapshots/{id}", axum::routing::delete(delete_snapshot))
        .route("/policies", get(list_policies))
        .route("/metrics/summary", get(metrics_summary))
        .route("/alerts", get(list_alerts))
        .route("/jobs", get(list_jobs))
        .route("/jobs/{id}", get(get_job))
        .route_layer(middleware::from_fn_with_state(
            state.clone(),
            auth_middleware,
        ))
        .with_state(state.clone());

    Router::new()
        .route("/health", get(health))
        .route("/version", get(version))
        .nest("/api/atlas/v1", api)
        .with_state(state)
}

// ---- meta ----

async fn health() -> Json<Value> {
    Json(json!({ "status": "ok" }))
}

async fn version() -> Json<Value> {
    Json(json!({
        "name": "atlas-gateway",
        "version": env!("CARGO_PKG_VERSION"),
        "api": "v1",
    }))
}

// ---- backends ----

async fn list_backends(State(s): State<AppState>) -> AppResult<Json<Vec<StorageBackend>>> {
    Ok(Json(atlas_inventory::list_backends(&s.pool).await?))
}

#[derive(Debug, Deserialize)]
struct CreateBackendBody {
    name: String,
    #[serde(default)]
    backend_type: Option<String>,
    #[serde(default)]
    mode: Option<String>,
}

/// Register a backend row. No cluster lifecycle happens here (PDF §8.1 keeps it 'pending').
async fn create_backend(
    State(s): State<AppState>,
    Extension(actor): Extension<Actor>,
    Json(body): Json<CreateBackendBody>,
) -> AppResult<Json<StorageBackend>> {
    if body.name.trim().is_empty() {
        return Err(AppError::Validation("name is required".into()));
    }
    let backend_type = match body.backend_type.as_deref() {
        None | Some("ceph") => BackendType::Ceph,
        Some("nfs") => BackendType::Nfs,
        Some("zfs") => BackendType::Zfs,
        Some("san") => BackendType::San,
        Some("cloud_block") => BackendType::CloudBlock,
        Some("kubernetes") => BackendType::Kubernetes,
        Some(other) => {
            return Err(AppError::Validation(format!(
                "unknown backend_type: {other}"
            )))
        }
    };
    let mode = match body.mode.as_deref() {
        Some("managed_rook") => BackendMode::ManagedRook,
        Some("read_only") => BackendMode::ReadOnly,
        _ => BackendMode::External,
    };
    let backend = StorageBackend {
        id: atlas_common::ids::backend_id(),
        name: body.name,
        backend_type,
        mode,
        status: "pending".into(),
        capabilities: ceph_default_caps(backend_type),
        connection_ref: None,
    };
    atlas_inventory::upsert_backend(&s.pool, &backend).await?;
    let _ = atlas_inventory::audit::record(
        &s.pool,
        None,
        &actor.id,
        "backend.create.succeeded",
        "backend",
        &backend.id,
        "success",
        Some(json!({ "name": backend.name, "type": backend.backend_type })),
        None,
    )
    .await;
    Ok(Json(backend))
}

/// Trigger a discovery pass for a registered backend (PDF §8.1).
async fn discover_backend(
    State(s): State<AppState>,
    Extension(actor): Extension<Actor>,
    Path(id): Path<String>,
) -> AppResult<Json<Value>> {
    let driver = s
        .driver_for(&id)
        .ok_or_else(|| AppError::NotFound(format!("no driver registered for backend {id}")))?;

    let result = atlas_discovery::run_discovery(&s.pool, driver).await;
    let (status, payload) = match &result {
        Ok(sum) => ("success", serde_json::to_value(sum).unwrap_or(Value::Null)),
        Err(e) => ("failed", json!({ "error": e.to_string() })),
    };
    let _ = atlas_inventory::audit::record(
        &s.pool,
        None,
        &actor.id,
        "backend.discover.requested",
        "backend",
        &id,
        status,
        None,
        Some(payload.clone()),
    )
    .await;

    let summary = result.map_err(AppError::from)?;
    Ok(Json(json!({ "state": "succeeded", "summary": summary })))
}

// ---- clusters / inventory ----

async fn list_clusters(State(s): State<AppState>) -> AppResult<Json<Value>> {
    Ok(Json(json!(atlas_inventory::list_clusters(&s.pool).await?)))
}

async fn cluster_health(
    State(s): State<AppState>,
    Path(id): Path<String>,
) -> AppResult<Json<Value>> {
    let h = atlas_inventory::cluster_health(&s.pool, &id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("cluster {id}")))?;
    Ok(Json(json!(h)))
}

async fn cluster_capabilities(
    State(s): State<AppState>,
    Path(id): Path<String>,
) -> AppResult<Json<Capabilities>> {
    let cluster = atlas_inventory::get_cluster(&s.pool, &id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("cluster {id}")))?;
    let backends = atlas_inventory::list_backends(&s.pool).await?;
    let caps = backends
        .into_iter()
        .find(|b| b.id == cluster.backend_id)
        .map(|b| b.capabilities)
        .unwrap_or_default();
    Ok(Json(caps))
}

/// Storage nodes are derived from distinct OSD hosts in the MVP.
async fn list_nodes(State(s): State<AppState>) -> AppResult<Json<Value>> {
    let osds = atlas_inventory::list_osds(&s.pool).await?;
    let mut hosts: Vec<String> = osds.into_iter().filter_map(|o| o.host).collect();
    hosts.sort();
    hosts.dedup();
    let nodes: Vec<Value> = hosts.into_iter().map(|h| json!({ "host": h })).collect();
    Ok(Json(json!(nodes)))
}

async fn list_osds(State(s): State<AppState>) -> AppResult<Json<Value>> {
    Ok(Json(json!(atlas_inventory::list_osds(&s.pool).await?)))
}

async fn list_pools(State(s): State<AppState>) -> AppResult<Json<Value>> {
    Ok(Json(json!(atlas_inventory::list_pools(&s.pool).await?)))
}

async fn list_volumes(State(s): State<AppState>) -> AppResult<Json<Value>> {
    Ok(Json(json!(atlas_inventory::list_volumes(&s.pool).await?)))
}

async fn get_volume(State(s): State<AppState>, Path(id): Path<String>) -> AppResult<Json<Value>> {
    let v = atlas_inventory::get_volume(&s.pool, &id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("volume {id}")))?;
    Ok(Json(json!(v)))
}

async fn metrics_summary(State(s): State<AppState>) -> AppResult<Json<Value>> {
    Ok(Json(atlas_inventory::metrics_summary(&s.pool).await?))
}

async fn list_alerts() -> Json<Value> {
    // Alert engine arrives with the monitor worker (a later slice).
    Json(json!([]))
}

// ---- jobs ----

async fn list_jobs(State(s): State<AppState>) -> AppResult<Json<Value>> {
    Ok(Json(json!(
        atlas_inventory::jobs::list_jobs(&s.pool, 100).await?
    )))
}

async fn get_job(State(s): State<AppState>, Path(id): Path<String>) -> AppResult<Json<Value>> {
    let job = atlas_inventory::jobs::get_job(&s.pool, &id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("job {id}")))?;
    Ok(Json(json!(job)))
}

// ---- policies ----

async fn list_policies() -> Json<Value> {
    let items: Vec<Value> = atlas_policy::POLICIES
        .iter()
        .map(|p| {
            json!({
                "intent": p.intent, "storage_class": p.storage_class,
                "access_mode": p.access_mode, "volume_mode": p.volume_mode,
                "description": p.description
            })
        })
        .collect();
    Json(json!(items))
}

// ---- snapshots (read) ----

async fn list_snapshots(State(s): State<AppState>) -> AppResult<Json<Value>> {
    Ok(Json(json!(
        atlas_inventory::snapshots::list_snapshots(&s.pool, None).await?
    )))
}

// ---- write path (async jobs; 202 Accepted + job id) ----

const CEPH_BACKEND_ID: &str = "bkd_ceph_lab";

fn accepted(job: &atlas_api_types::JobRecord, resource: Value) -> (StatusCode, Json<Value>) {
    (
        StatusCode::ACCEPTED,
        Json(json!({
            "job_id": job.id,
            "state": job.state,
            "resource": resource,
            "links": { "job": format!("/api/atlas/v1/jobs/{}", job.id) }
        })),
    )
}

/// `POST /volumes` — create a Ceph-backed PVC as an async job (PDF §8.2).
async fn create_volume(
    State(s): State<AppState>,
    Extension(actor): Extension<Actor>,
    Json(body): Json<CreateVolumeRequest>,
) -> AppResult<(StatusCode, Json<Value>)> {
    if body.name.trim().is_empty() {
        return Err(AppError::Validation("name is required".into()));
    }
    if body.size_bytes <= 0 {
        return Err(AppError::Validation("size_bytes must be > 0".into()));
    }

    let k8s_opts = body.kubernetes.clone();
    let namespace = k8s_opts
        .as_ref()
        .and_then(|k| k.namespace.clone())
        .unwrap_or_else(|| "default".into());
    let sc_override = k8s_opts.as_ref().and_then(|k| k.storage_class.clone());
    let placement =
        atlas_policy::resolve(body.policy.as_deref(), body.kind, sc_override.as_deref());
    let access_modes = k8s_opts
        .as_ref()
        .map(|k| k.access_modes.clone())
        .filter(|m| !m.is_empty());
    let access_mode = access_modes
        .and_then(|m| m.into_iter().next())
        .unwrap_or_else(|| placement.access_mode.clone());
    let volume_mode = k8s_opts
        .as_ref()
        .and_then(|k| k.volume_mode.clone())
        .unwrap_or_else(|| placement.volume_mode.clone());

    let volume_id = ids::volume_id();
    let job_id = ids::job_id();
    let owner = body.owner.clone().map(|o| OwnerRef {
        product: o.product,
        resource_type: o.resource_type,
        resource_id: o.resource_id,
        role: o.role,
    });
    // Idempotency: a repeat of the same tenant/name/size create returns the same job (PDF §17.4).
    let idem = ids::stable_id(
        "idem",
        &format!(
            "{}|{}|{}|volume.create",
            body.tenant_id, body.name, body.size_bytes
        ),
    );

    let spec = JobSpec::VolumeCreate {
        volume_id: volume_id.clone(),
        backend_id: CEPH_BACKEND_ID.into(),
        name: body.name.clone(),
        namespace: namespace.clone(),
        storage_class: placement.storage_class.clone(),
        access_mode,
        volume_mode,
        size_bytes: body.size_bytes,
        kind: format!("{:?}", body.kind).to_lowercase(),
        policy: Some(placement.intent.clone()),
        owner,
    };

    let job = s
        .jobs
        .enqueue(&job_id, &body.tenant_id, &actor.id, spec, Some(&idem))
        .await
        .map_err(AppError::from)?;

    let _ = atlas_inventory::audit::record(
        &s.pool,
        Some(&body.tenant_id),
        &actor.id,
        "volume.create.requested",
        "volume",
        &volume_id,
        "accepted",
        Some(json!({ "name": body.name, "policy": placement.intent, "storage_class": placement.storage_class })),
        None,
    )
    .await;

    Ok(accepted(
        &job,
        json!({ "volume_id": volume_id, "storage_class": placement.storage_class,
                "namespace": namespace, "pvc": body.name }),
    ))
}

/// `DELETE /volumes/{id}` — delete the PVC + inventory row as a job.
async fn delete_volume(
    State(s): State<AppState>,
    Extension(actor): Extension<Actor>,
    Path(id): Path<String>,
) -> AppResult<(StatusCode, Json<Value>)> {
    let vol = atlas_inventory::get_volume(&s.pool, &id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("volume {id}")))?;
    let namespace = vol
        .kubernetes_namespace
        .ok_or_else(|| AppError::Validation("volume has no kubernetes namespace".into()))?;
    let pvc_name = vol
        .pvc_name
        .ok_or_else(|| AppError::Validation("volume has no pvc".into()))?;

    let job_id = ids::job_id();
    let spec = JobSpec::VolumeDelete {
        volume_id: id.clone(),
        namespace,
        pvc_name,
    };
    let job = s
        .jobs
        .enqueue(&job_id, "global", &actor.id, spec, None)
        .await
        .map_err(AppError::from)?;

    let _ = atlas_inventory::audit::record(
        &s.pool,
        None,
        &actor.id,
        "volume.delete.requested",
        "volume",
        &id,
        "accepted",
        None,
        None,
    )
    .await;
    Ok(accepted(&job, json!({ "volume_id": id })))
}

#[derive(Debug, Deserialize)]
struct ExpandBody {
    new_size_bytes: i64,
}

/// `POST /volumes/{id}/expand`
async fn expand_volume(
    State(s): State<AppState>,
    Extension(actor): Extension<Actor>,
    Path(id): Path<String>,
    Json(body): Json<ExpandBody>,
) -> AppResult<(StatusCode, Json<Value>)> {
    let vol = atlas_inventory::get_volume(&s.pool, &id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("volume {id}")))?;
    if body.new_size_bytes <= vol.size_bytes {
        return Err(AppError::Validation(
            "new_size_bytes must be larger than the current size".into(),
        ));
    }
    let namespace = vol
        .kubernetes_namespace
        .ok_or_else(|| AppError::Validation("volume has no kubernetes namespace".into()))?;
    let pvc_name = vol
        .pvc_name
        .ok_or_else(|| AppError::Validation("volume has no pvc".into()))?;

    let job_id = ids::job_id();
    let spec = JobSpec::VolumeExpand {
        volume_id: id.clone(),
        namespace,
        pvc_name,
        new_size_bytes: body.new_size_bytes,
    };
    let job = s
        .jobs
        .enqueue(&job_id, "global", &actor.id, spec, None)
        .await
        .map_err(AppError::from)?;
    Ok(accepted(
        &job,
        json!({ "volume_id": id, "new_size_bytes": body.new_size_bytes }),
    ))
}

#[derive(Debug, Deserialize)]
struct SnapshotBody {
    name: Option<String>,
    snapshot_class: Option<String>,
}

/// `POST /volumes/{id}/snapshots`
async fn create_snapshot(
    State(s): State<AppState>,
    Extension(actor): Extension<Actor>,
    Path(id): Path<String>,
    Json(body): Json<SnapshotBody>,
) -> AppResult<(StatusCode, Json<Value>)> {
    let vol = atlas_inventory::get_volume(&s.pool, &id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("volume {id}")))?;
    let namespace = vol
        .kubernetes_namespace
        .ok_or_else(|| AppError::Validation("volume has no kubernetes namespace".into()))?;
    let pvc_name = vol
        .pvc_name
        .ok_or_else(|| AppError::Validation("volume has no pvc".into()))?;

    let snapshot_id = ids::snapshot_id();
    let snap_name = body
        .name
        .unwrap_or_else(|| format!("{}-{}", vol.name, &snapshot_id[5..]));
    let snapshot_class = body
        .snapshot_class
        .unwrap_or_else(|| "zyvor-rbd-snapclass".into());

    let job_id = ids::job_id();
    let spec = JobSpec::SnapshotCreate {
        snapshot_id: snapshot_id.clone(),
        volume_id: id.clone(),
        name: snap_name.clone(),
        namespace,
        pvc_name,
        snapshot_class,
    };
    let job = s
        .jobs
        .enqueue(&job_id, "global", &actor.id, spec, None)
        .await
        .map_err(AppError::from)?;

    let _ = atlas_inventory::audit::record(
        &s.pool,
        None,
        &actor.id,
        "snapshot.create.requested",
        "volume",
        &id,
        "accepted",
        Some(json!({ "snapshot_id": snapshot_id, "name": snap_name })),
        None,
    )
    .await;
    Ok(accepted(
        &job,
        json!({ "snapshot_id": snapshot_id, "volume_id": id, "name": snap_name }),
    ))
}

/// `DELETE /snapshots/{id}`
async fn delete_snapshot(
    State(s): State<AppState>,
    Extension(actor): Extension<Actor>,
    Path(id): Path<String>,
) -> AppResult<(StatusCode, Json<Value>)> {
    let snap = atlas_inventory::snapshots::get_snapshot(&s.pool, &id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("snapshot {id}")))?;
    // The VolumeSnapshot lives in the source volume's namespace.
    let vol = atlas_inventory::get_volume(&s.pool, &snap.volume_id).await?;
    let namespace = vol
        .and_then(|v| v.kubernetes_namespace)
        .unwrap_or_else(|| "default".into());

    let job_id = ids::job_id();
    let spec = JobSpec::SnapshotDelete {
        snapshot_id: id.clone(),
        namespace,
        name: snap.name,
    };
    let job = s
        .jobs
        .enqueue(&job_id, &snap.tenant_id, &actor.id, spec, None)
        .await
        .map_err(AppError::from)?;
    Ok(accepted(&job, json!({ "snapshot_id": id })))
}

// ---- live Kubernetes ----

async fn list_storage_classes(State(s): State<AppState>) -> AppResult<Json<Vec<StorageClassInfo>>> {
    let k8s = s
        .k8s
        .as_ref()
        .ok_or_else(|| AppError::Driver("no reachable Kubernetes cluster".into()))?;
    let classes = k8s
        .list_storage_classes()
        .await
        .map_err(|e| AppError::Driver(e.to_string()))?;
    Ok(Json(classes))
}

async fn list_pvcs(State(s): State<AppState>) -> AppResult<Json<Value>> {
    let k8s = s
        .k8s
        .as_ref()
        .ok_or_else(|| AppError::Driver("no reachable Kubernetes cluster".into()))?;
    let pvcs = k8s
        .list_pvcs(None)
        .await
        .map_err(|e| AppError::Driver(e.to_string()))?;
    Ok(Json(json!(pvcs)))
}

async fn list_pvs(State(s): State<AppState>) -> AppResult<Json<Value>> {
    let k8s = s
        .k8s
        .as_ref()
        .ok_or_else(|| AppError::Driver("no reachable Kubernetes cluster".into()))?;
    let pvs = k8s
        .list_pvs()
        .await
        .map_err(|e| AppError::Driver(e.to_string()))?;
    Ok(Json(json!(pvs)))
}

// ---- helpers ----

fn ceph_default_caps(t: BackendType) -> Capabilities {
    match t {
        BackendType::Ceph => Capabilities {
            block: true,
            file: true,
            object: true,
            snapshots: true,
            clone: true,
            expansion: true,
            replication: true,
        },
        _ => Capabilities::default(),
    }
}
