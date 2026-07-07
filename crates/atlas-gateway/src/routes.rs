// Copyright (c) 2026 ZyvorAI Labs Private Limited. All rights reserved.
//! REST surface. Slice 1 read-only inventory/discovery (PDF §10.2) + slice 2 async write path
//! (volume create/expand/delete, snapshots) — write ops return `202 Accepted` with a job id.

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    middleware,
    routing::{get, post},
    Extension, Json, Router,
};
use serde::Deserialize;
use serde_json::{json, Value};

use atlas_api_types::{
    BackendMode, BackendType, Capabilities, CreateVolumeRequest, Owner, StorageBackend,
    StorageClassInfo,
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
        .route("/snapshots/{id}/clone", post(clone_snapshot))
        .route("/snapshots/{id}/restore", post(restore_snapshot))
        .route("/buckets", get(list_buckets).post(create_bucket))
        .route("/buckets/{id}", get(get_bucket))
        .route("/backup-jobs", post(create_backup))
        .route("/restore-jobs", post(create_restore))
        .route("/backups", get(list_backups))
        .route("/backups/{id}", get(get_backup))
        .route("/policies", get(list_policies))
        .route("/metrics/summary", get(metrics_summary))
        .route("/metrics/ceph", get(metrics_ceph))
        .route("/alerts", get(list_alerts))
        .route("/alerts/evaluate", post(evaluate_alerts))
        .route("/jobs", get(list_jobs))
        .route("/jobs/{id}", get(get_job))
        .route("/jobs/{id}/watch", get(watch_job_sse))
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
    crate::auth::require_role(s.config.auth_required, &actor, crate::auth::ROLE_ADMIN)?;
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
    crate::auth::require_role(s.config.auth_required, &actor, crate::auth::ROLE_OPERATOR)?;
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

#[derive(Debug, Deserialize)]
struct MetricQuery {
    prefix: Option<String>,
}

/// `GET /metrics/ceph[?prefix=ceph_osd]` — latest Ceph metrics scraped from the mgr Prometheus module.
async fn metrics_ceph(
    State(s): State<AppState>,
    Query(q): Query<MetricQuery>,
) -> AppResult<Json<Value>> {
    Ok(Json(json!(
        atlas_inventory::metrics::list(&s.pool, q.prefix.as_deref()).await?
    )))
}

#[derive(Debug, Deserialize)]
struct AlertQuery {
    state: Option<String>,
}

/// `GET /alerts[?state=open]` — alerts produced by the monitor worker (PDF §15.2).
async fn list_alerts(
    State(s): State<AppState>,
    Query(q): Query<AlertQuery>,
) -> AppResult<Json<Value>> {
    Ok(Json(json!(
        atlas_inventory::alerts::list(&s.pool, q.state.as_deref()).await?
    )))
}

/// `POST /alerts/evaluate` — run the alert rules on demand (also runs on the monitor interval).
async fn evaluate_alerts(State(s): State<AppState>) -> AppResult<Json<Value>> {
    atlas_monitor::evaluate(&s.pool).await?;
    let open = atlas_inventory::alerts::count_open(&s.pool).await?;
    Ok(Json(json!({ "evaluated": true, "open_alerts": open })))
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

/// `GET /jobs/{id}/watch` — Server-Sent Events; emits the job on each state change until terminal
/// (REST parity with the gRPC `WatchJob` stream).
async fn watch_job_sse(
    State(s): State<AppState>,
    Path(id): Path<String>,
) -> axum::response::sse::Sse<
    impl tokio_stream::Stream<Item = Result<axum::response::sse::Event, std::convert::Infallible>>,
> {
    use axum::response::sse::{Event, KeepAlive, Sse};
    let pool = s.pool.clone();
    let stream = async_stream::stream! {
        let mut last = String::new();
        for _ in 0..240 {
            match atlas_inventory::jobs::get_job(&pool, &id).await {
                Ok(Some(j)) => {
                    let terminal = j.state == "succeeded" || j.state == "failed";
                    if j.state != last {
                        last = j.state.clone();
                        let data = serde_json::to_string(&j).unwrap_or_default();
                        yield Ok(Event::default().event("job").data(data));
                    }
                    if terminal {
                        break;
                    }
                }
                Ok(None) => {
                    yield Ok(Event::default().event("error").data(format!("job {id} not found")));
                    break;
                }
                Err(e) => {
                    yield Ok(Event::default().event("error").data(e.to_string()));
                    break;
                }
            }
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        }
    };
    Sse::new(stream).keep_alive(KeepAlive::default())
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
    crate::auth::require_role(s.config.auth_required, &actor, crate::auth::ROLE_OPERATOR)?;
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
    crate::auth::require_role(s.config.auth_required, &actor, crate::auth::ROLE_ADMIN)?;
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
    crate::auth::require_role(s.config.auth_required, &actor, crate::auth::ROLE_OPERATOR)?;
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
    crate::auth::require_role(s.config.auth_required, &actor, crate::auth::ROLE_OPERATOR)?;
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

#[derive(Debug, Deserialize)]
struct ForceParams {
    #[serde(default)]
    force: bool,
}

/// `DELETE /snapshots/{id}[?force=true]` — blocked if the snapshot has dependent clones (PDF §8.3).
async fn delete_snapshot(
    State(s): State<AppState>,
    Extension(actor): Extension<Actor>,
    Path(id): Path<String>,
    Query(q): Query<ForceParams>,
) -> AppResult<(StatusCode, Json<Value>)> {
    crate::auth::require_role(s.config.auth_required, &actor, crate::auth::ROLE_OPERATOR)?;
    // Force-deleting past the dependency guard is an admin action.
    if q.force {
        crate::auth::require_role(s.config.auth_required, &actor, crate::auth::ROLE_ADMIN)?;
    }
    let snap = atlas_inventory::snapshots::get_snapshot(&s.pool, &id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("snapshot {id}")))?;

    // Safe-by-default: refuse to delete a snapshot that still has clones/restores derived from it.
    let deps = atlas_inventory::count_snapshot_dependents(&s.pool, &id).await?;
    if deps > 0 && !q.force {
        return Err(AppError::Conflict(format!(
            "snapshot {id} has {deps} dependent volume(s); delete them first or pass ?force=true"
        )));
    }

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
    Ok(accepted(
        &job,
        json!({ "snapshot_id": id, "forced": q.force }),
    ))
}

#[derive(Debug, Deserialize)]
struct CloneBody {
    /// New volume/PVC name (required for clone; optional for restore).
    name: Option<String>,
    namespace: Option<String>,
    storage_class: Option<String>,
    size_bytes: Option<i64>,
    #[serde(default)]
    owner: Option<Owner>,
}

/// `POST /snapshots/{id}/clone` — provision a new independent volume from a snapshot (PDF §8.3).
async fn clone_snapshot(
    State(s): State<AppState>,
    Extension(actor): Extension<Actor>,
    Path(id): Path<String>,
    Json(body): Json<CloneBody>,
) -> AppResult<(StatusCode, Json<Value>)> {
    let name = body
        .name
        .clone()
        .filter(|n| !n.trim().is_empty())
        .ok_or_else(|| AppError::Validation("name is required for clone".into()))?;
    enqueue_clone(&s, &actor, &id, "clone", name, body).await
}

/// `POST /snapshots/{id}/restore` — provision a point-in-time copy of the source volume (PDF §8.3).
async fn restore_snapshot(
    State(s): State<AppState>,
    Extension(actor): Extension<Actor>,
    Path(id): Path<String>,
    Json(body): Json<CloneBody>,
) -> AppResult<(StatusCode, Json<Value>)> {
    // Default restore name derives from the snapshot id when not supplied.
    let name = body
        .name
        .clone()
        .filter(|n| !n.trim().is_empty())
        .unwrap_or_else(|| format!("restore-{}", &id[5..]));
    enqueue_clone(&s, &actor, &id, "restore", name, body).await
}

async fn enqueue_clone(
    s: &AppState,
    actor: &Actor,
    snapshot_id: &str,
    mode: &str,
    new_name: String,
    body: CloneBody,
) -> AppResult<(StatusCode, Json<Value>)> {
    crate::auth::require_role(s.config.auth_required, actor, crate::auth::ROLE_OPERATOR)?;
    let snap = atlas_inventory::snapshots::get_snapshot(&s.pool, snapshot_id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("snapshot {snapshot_id}")))?;
    // Defaults come from the source volume.
    let src = atlas_inventory::get_volume(&s.pool, &snap.volume_id).await?;
    let namespace = body
        .namespace
        .or_else(|| src.as_ref().and_then(|v| v.kubernetes_namespace.clone()))
        .unwrap_or_else(|| "default".into());
    let storage_class = body
        .storage_class
        .or_else(|| src.as_ref().and_then(|v| v.storage_class_name.clone()))
        .unwrap_or_else(|| atlas_policy::DEFAULT_BLOCK_SC.to_string());
    let size_bytes = body
        .size_bytes
        .or_else(|| src.as_ref().map(|v| v.size_bytes))
        .ok_or_else(|| {
            AppError::Validation("size_bytes is required (source volume unknown)".into())
        })?;

    let new_volume_id = ids::volume_id();
    let job_id = ids::job_id();
    let owner = body.owner.map(|o| OwnerRef {
        product: o.product,
        resource_type: o.resource_type,
        resource_id: o.resource_id,
        role: o.role,
    });

    let spec = JobSpec::SnapshotClone {
        mode: mode.into(),
        new_volume_id: new_volume_id.clone(),
        backend_id: CEPH_BACKEND_ID.into(),
        snapshot_id: snapshot_id.to_string(),
        snapshot_k8s_name: snap.name.clone(),
        new_name: new_name.clone(),
        namespace: namespace.clone(),
        storage_class: storage_class.clone(),
        size_bytes,
        access_mode: "ReadWriteOnce".into(),
        volume_mode: "Filesystem".into(),
        owner,
    };
    let job = s
        .jobs
        .enqueue(&job_id, &snap.tenant_id, &actor.id, spec, None)
        .await
        .map_err(AppError::from)?;

    let _ = atlas_inventory::audit::record(
        &s.pool,
        None,
        &actor.id,
        &format!("snapshot.{mode}.requested"),
        "snapshot",
        snapshot_id,
        "accepted",
        Some(json!({ "new_volume_id": new_volume_id, "new_name": new_name })),
        None,
    )
    .await;

    Ok(accepted(
        &job,
        json!({ "volume_id": new_volume_id, "from_snapshot": snapshot_id,
                "mode": mode, "namespace": namespace, "pvc": new_name,
                "storage_class": storage_class }),
    ))
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

// ---- object storage (buckets) ----

async fn list_buckets(State(s): State<AppState>) -> AppResult<Json<Value>> {
    Ok(Json(json!(
        atlas_inventory::buckets::list_buckets(&s.pool).await?
    )))
}

async fn get_bucket(State(s): State<AppState>, Path(id): Path<String>) -> AppResult<Json<Value>> {
    let b = atlas_inventory::buckets::get_bucket(&s.pool, &id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("bucket {id}")))?;
    Ok(Json(json!(b)))
}

#[derive(Debug, Deserialize)]
struct CreateBucketBody {
    name: String,
    namespace: Option<String>,
    storage_class: Option<String>,
}

/// `POST /buckets` — provision an RGW bucket via an ObjectBucketClaim (async job).
async fn create_bucket(
    State(s): State<AppState>,
    Extension(actor): Extension<Actor>,
    Json(body): Json<CreateBucketBody>,
) -> AppResult<(StatusCode, Json<Value>)> {
    crate::auth::require_role(s.config.auth_required, &actor, crate::auth::ROLE_OPERATOR)?;
    if body.name.trim().is_empty() {
        return Err(AppError::Validation("name is required".into()));
    }
    // The OBC (and its Secret/ConfigMap) live where this gateway can read them.
    let namespace = body.namespace.unwrap_or_else(|| "rook-ceph".into());
    let storage_class = body
        .storage_class
        .unwrap_or_else(|| "zyvor-rgw-bucket".into());
    let bucket_id = ids::bucket_id();
    let obc_name = body.name.clone();

    atlas_inventory::buckets::insert_bucket(
        &s.pool, &bucket_id, "global", &body.name, &namespace, &obc_name,
    )
    .await?;

    let job_id = ids::job_id();
    let spec = JobSpec::BucketCreate {
        bucket_id: bucket_id.clone(),
        namespace: namespace.clone(),
        obc_name,
        storage_class,
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
        "bucket.create.requested",
        "bucket",
        &bucket_id,
        "accepted",
        Some(json!({ "name": body.name })),
        None,
    )
    .await;
    Ok(accepted(
        &job,
        json!({ "bucket_id": bucket_id, "namespace": namespace }),
    ))
}

// ---- backups ----

async fn list_backups(State(s): State<AppState>) -> AppResult<Json<Value>> {
    Ok(Json(json!(
        atlas_inventory::backups::list_backups(&s.pool, None).await?
    )))
}

async fn get_backup(State(s): State<AppState>, Path(id): Path<String>) -> AppResult<Json<Value>> {
    let b = atlas_inventory::backups::get_backup(&s.pool, &id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("backup {id}")))?;
    Ok(Json(json!(b)))
}

#[derive(Debug, Deserialize)]
struct CreateBackupBody {
    volume_id: String,
    bucket_id: String,
    /// "manifest" (default) or "data" (also exports the RBD image data to S3).
    #[serde(default)]
    mode: Option<String>,
}

/// `POST /backup-jobs` — snapshot a volume and write a backup manifest to an RGW bucket (PDF §16).
async fn create_backup(
    State(s): State<AppState>,
    Extension(actor): Extension<Actor>,
    Json(body): Json<CreateBackupBody>,
) -> AppResult<(StatusCode, Json<Value>)> {
    crate::auth::require_role(s.config.auth_required, &actor, crate::auth::ROLE_OPERATOR)?;
    let vol = atlas_inventory::get_volume(&s.pool, &body.volume_id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("volume {}", body.volume_id)))?;
    let volume_namespace = vol
        .kubernetes_namespace
        .clone()
        .ok_or_else(|| AppError::Validation("volume has no kubernetes namespace".into()))?;
    let pvc_name = vol
        .pvc_name
        .clone()
        .ok_or_else(|| AppError::Validation("volume has no pvc".into()))?;

    let bucket = atlas_inventory::buckets::get_bucket(&s.pool, &body.bucket_id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("bucket {}", body.bucket_id)))?;
    if bucket.state != "bound" {
        return Err(AppError::Validation(format!(
            "bucket {} is not bound yet (state: {})",
            bucket.id, bucket.state
        )));
    }
    let bucket_endpoint = bucket
        .endpoint
        .ok_or_else(|| AppError::Validation("bucket has no endpoint".into()))?;
    let bucket_name = bucket
        .bucket_name
        .ok_or_else(|| AppError::Validation("bucket has no bucket_name".into()))?;
    let bucket_secret_ref = bucket
        .secret_ref
        .ok_or_else(|| AppError::Validation("bucket has no secret ref".into()))?;
    let bucket_namespace = bucket.namespace.unwrap_or_else(|| "rook-ceph".into());
    let bucket_region = bucket.region.unwrap_or_else(|| "us-east-1".into());

    let backup_id = ids::stable_id("bkp", &format!("{}-{}", body.volume_id, ids::job_id()));
    let snapshot_id = ids::snapshot_id();
    let snapshot_name = format!("{pvc_name}-bkp-{}", &backup_id[4..]);
    let object_key = format!("backups/{}/{}.manifest.json", body.volume_id, backup_id);

    let manifest = json!({
        "backup_id": backup_id,
        "source_volume": body.volume_id,
        "source_snapshot": snapshot_id,
        "pvc": format!("{volume_namespace}/{pvc_name}"),
        "object_key": object_key,
        "format": "manifest-v1",
        "created_at": chrono::Utc::now().to_rfc3339(),
    });

    // Record the point-in-time snapshot + the pending backup.
    atlas_inventory::snapshots::insert_snapshot(
        &s.pool,
        &snapshot_id,
        "global",
        &body.volume_id,
        &snapshot_name,
        None,
        "app",
        "creating",
    )
    .await?;
    atlas_inventory::backups::insert_backup(
        &s.pool,
        &backup_id,
        "global",
        &body.volume_id,
        Some(&snapshot_id),
        &body.bucket_id,
        &object_key,
        "manifest-v1",
        &manifest,
    )
    .await?;

    let job_id = ids::job_id();
    let spec = JobSpec::BackupCreate {
        backup_id: backup_id.clone(),
        snapshot_id,
        volume_namespace,
        pvc_name,
        snapshot_name,
        snapshot_class: "zyvor-rbd-snapclass".into(),
        object_key: object_key.clone(),
        manifest_json: manifest.to_string(),
        bucket_namespace,
        bucket_secret_ref,
        bucket_endpoint,
        bucket_name,
        bucket_region,
        mode: body.mode.unwrap_or_else(|| "manifest".into()),
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
        "backup.create.requested",
        "volume",
        &body.volume_id,
        "accepted",
        Some(json!({ "backup_id": backup_id, "bucket_id": body.bucket_id })),
        None,
    )
    .await;
    Ok(accepted(
        &job,
        json!({ "backup_id": backup_id, "object_key": object_key, "bucket_id": body.bucket_id }),
    ))
}

#[derive(Debug, Deserialize)]
struct CreateRestoreBody {
    backup_id: String,
    /// New volume/PVC name; defaults to `restore-<backup-suffix>`.
    name: Option<String>,
    storage_class: Option<String>,
}

/// `POST /restore-jobs` — restore a volume from a backup: verify the manifest in RGW, then
/// provision a new PVC from the backup's snapshot (PDF §16, DR-2).
async fn create_restore(
    State(s): State<AppState>,
    Extension(actor): Extension<Actor>,
    Json(body): Json<CreateRestoreBody>,
) -> AppResult<(StatusCode, Json<Value>)> {
    crate::auth::require_role(s.config.auth_required, &actor, crate::auth::ROLE_OPERATOR)?;
    let backup = atlas_inventory::backups::get_backup(&s.pool, &body.backup_id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("backup {}", body.backup_id)))?;
    let snapshot_id = backup
        .snapshot_id
        .clone()
        .ok_or_else(|| AppError::Validation("backup has no snapshot to restore from".into()))?;
    // The backup's VolumeSnapshot (k8s object) + its namespace come from the snapshot's source volume.
    let snap = atlas_inventory::snapshots::get_snapshot(&s.pool, &snapshot_id)
        .await?
        .ok_or_else(|| AppError::Validation("backup snapshot no longer exists".into()))?;
    let src = atlas_inventory::get_volume(&s.pool, &snap.volume_id).await?;
    let namespace = src
        .as_ref()
        .and_then(|v| v.kubernetes_namespace.clone())
        .unwrap_or_else(|| "default".into());
    let storage_class = body
        .storage_class
        .or_else(|| src.as_ref().and_then(|v| v.storage_class_name.clone()))
        .unwrap_or_else(|| atlas_policy::DEFAULT_BLOCK_SC.to_string());
    let size_bytes = src.as_ref().map(|v| v.size_bytes).unwrap_or(1_073_741_824);

    // Bucket details to read + verify the manifest.
    let bucket = atlas_inventory::buckets::get_bucket(&s.pool, &backup.bucket_id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("bucket {}", backup.bucket_id)))?;
    let bucket_endpoint = bucket.endpoint.unwrap_or_default();
    let bucket_name = bucket.bucket_name.unwrap_or_default();
    let bucket_secret_ref = bucket.secret_ref.unwrap_or_default();
    let bucket_namespace = bucket.namespace.unwrap_or_else(|| "rook-ceph".into());
    let bucket_region = bucket.region.unwrap_or_else(|| "us-east-1".into());

    let new_volume_id = ids::volume_id();
    let new_name = body
        .name
        .filter(|n| !n.trim().is_empty())
        .unwrap_or_else(|| format!("restore-{}", &backup.id[4..]));
    let job_id = ids::job_id();

    let spec = JobSpec::RestoreBackup {
        backup_id: backup.id.clone(),
        new_volume_id: new_volume_id.clone(),
        backend_id: CEPH_BACKEND_ID.into(),
        snapshot_id,
        snapshot_k8s_name: snap.name,
        new_name: new_name.clone(),
        namespace: namespace.clone(),
        storage_class,
        size_bytes,
        object_key: backup.object_key.clone(),
        expected_checksum: backup.checksum.unwrap_or_default(),
        bucket_namespace,
        bucket_secret_ref,
        bucket_endpoint,
        bucket_name,
        bucket_region,
    };
    let job = s
        .jobs
        .enqueue(&job_id, &backup.tenant_id, &actor.id, spec, None)
        .await
        .map_err(AppError::from)?;
    let _ = atlas_inventory::audit::record(
        &s.pool,
        None,
        &actor.id,
        "backup.restore.requested",
        "backup",
        &backup.id,
        "accepted",
        Some(json!({ "new_volume_id": new_volume_id })),
        None,
    )
    .await;
    Ok(accepted(
        &job,
        json!({ "volume_id": new_volume_id, "from_backup": backup.id,
                "namespace": namespace, "pvc": new_name }),
    ))
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
