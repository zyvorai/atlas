// Copyright (c) 2026 ZyvorAI Labs Private Limited. All rights reserved.
//! Read-only REST surface for MVP slice 1 (PDF §10.2). The write path (POST /volumes, snapshots,
//! clones) lands in slice 2; those routes are intentionally absent here.

use axum::{
    extract::{Path, State},
    middleware,
    routing::{get, post},
    Extension, Json, Router,
};
use serde::Deserialize;
use serde_json::{json, Value};

use atlas_api_types::{BackendMode, BackendType, Capabilities, StorageBackend, StorageClassInfo};
use atlas_common::{AppError, AppResult};

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
        .route("/volumes", get(list_volumes))
        .route("/volumes/{id}", get(get_volume))
        .route("/metrics/summary", get(metrics_summary))
        .route("/alerts", get(list_alerts))
        .route("/jobs", get(list_jobs))
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
    // Alert engine arrives with the monitor worker (slice 2).
    Json(json!([]))
}

async fn list_jobs() -> Json<Value> {
    // Job engine arrives in slice 2 (write path).
    Json(json!([]))
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
