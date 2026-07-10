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
        .route("/backends/summary", get(backends_summary))
        .route("/backends/{id}/discover", post(discover_backend))
        .route("/clusters", get(list_clusters))
        .route("/clusters/{id}/health", get(cluster_health))
        .route("/clusters/{id}/capabilities", get(cluster_capabilities))
        .route("/nodes", get(list_nodes))
        .route("/osds", get(list_osds))
        .route("/pools", get(list_pools))
        .route("/ceph/status", get(get_ceph_status))
        .route("/ceph/osd-tree", get(get_ceph_osd_tree))
        .route("/ceph/osd-df", get(get_ceph_osd_df))
        .route("/ceph/df", get(get_ceph_df))
        .route("/storage-classes", get(list_storage_classes))
        .route("/kubernetes/pvcs", get(list_pvcs))
        .route("/kubernetes/pvs", get(list_pvs))
        .route("/volumes", get(list_volumes).post(create_volume))
        .route("/volumes.csv", get(volumes_csv))
        .route("/volumes/{id}", get(get_volume).delete(delete_volume))
        .route("/volumes/{id}/expand", post(expand_volume))
        .route("/rbd-images", get(list_rbd_images).post(create_rbd_image))
        .route(
            "/rbd-images/{pool}/{image}",
            axum::routing::delete(delete_rbd_image),
        )
        .route("/rbd-images/{pool}/{image}/clone", post(clone_rbd_image))
        .route("/rbd-images/{pool}/{image}/resize", post(resize_rbd_image))
        .route(
            "/rbd-images/{pool}/{image}/flatten",
            post(flatten_rbd_image),
        )
        .route(
            "/rbd-images/{pool}/{image}/snapshots",
            get(list_rbd_snaps).post(create_rbd_snap),
        )
        .route(
            "/rbd-images/{pool}/{image}/rollback",
            post(rollback_rbd_image),
        )
        .route("/rbd-usage/refresh", post(refresh_rbd_usage))
        .route("/buckets/{id}/stats", get(bucket_stats))
        .route(
            "/buckets/{id}/objects",
            get(bucket_objects).delete(bucket_object_delete),
        )
        .route("/buckets/{id}/objects/upload-url", post(bucket_object_upload_url))
        .route("/buckets/{id}/objects/download-url", get(bucket_object_download_url))
        .route("/buckets/{id}/objects/prune", post(bucket_objects_prune))
        .route("/volumes/{id}/bindings", get(list_volume_bindings))
        .route(
            "/volumes/{id}/labels",
            get(get_volume_labels).put(put_volume_labels),
        )
        .route("/tenants", get(list_tenants))
        .route("/volumes/{id}/snapshots", post(create_snapshot))
        .route("/snapshots", get(list_snapshots))
        .route("/snapshots/{id}", axum::routing::delete(delete_snapshot))
        .route("/snapshots/{id}/clone", post(clone_snapshot))
        .route("/snapshots/{id}/restore", post(restore_snapshot))
        .route("/volumes/{id}/schedule", post(create_schedule))
        .route("/schedules", get(list_schedules))
        .route("/schedules/{id}", axum::routing::delete(delete_schedule))
        .route("/buckets", get(list_buckets).post(create_bucket))
        .route("/buckets/{id}", get(get_bucket).delete(delete_bucket))
        .route("/backup-jobs", post(create_backup))
        .route("/restore-jobs", post(create_restore))
        .route("/backups", get(list_backups))
        .route("/backups/{id}", get(get_backup).delete(delete_backup))
        .route("/backups/{id}/download", get(download_backup))
        // ---- DataBridge (cloud-to-edge DB migration) ----
        .route(
            "/databridge/sources",
            get(db_list_sources).post(db_create_source),
        )
        .route(
            "/databridge/sources/{id}",
            get(db_get_source).delete(db_delete_source),
        )
        .route("/databridge/sources/{id}/discover", post(db_discover_source))
        .route("/databridge/plans", get(db_list_plans).post(db_create_plan))
        .route("/databridge/plans/{id}", get(db_get_plan).delete(db_delete_plan))
        .route("/databridge/plans/{id}/assess", post(db_assess_plan))
        .route("/databridge/plans/{id}/provision", post(db_provision_edge))
        .route("/databridge/plans/{id}/full-load", post(db_full_load))
        .route("/databridge/plans/{id}/cdc/start", post(db_cdc_start))
        .route("/databridge/plans/{id}/cdc/stop", post(db_cdc_stop))
        .route("/databridge/plans/{id}/validate", post(db_validate))
        .route("/databridge/plans/{id}/cutover", post(db_cutover))
        .route("/databridge/plans/{id}/rollback", post(db_rollback))
        .route("/databridge/edge-clusters", get(db_list_edge_clusters))
        .route("/databridge/edge-clusters/{id}", get(db_get_edge_cluster))
        .route("/databridge/cdc-streams", get(db_list_cdc_streams))
        .route("/databridge/cdc-streams/{id}", get(db_get_cdc_stream))
        .route("/databridge/validations", get(db_list_validations))
        .route("/databridge/cutovers", get(db_list_cutovers))
        .route("/policies", get(list_policies))
        .route("/auth/tokens", post(issue_token))
        .route(
            "/tenants/{id}/quota",
            get(get_tenant_quota).put(put_tenant_quota),
        )
        .route("/tenants/{id}/policies", get(list_tenant_policies))
        .route(
            "/tenants/{id}/policies/{intent}",
            axum::routing::put(put_tenant_policy).delete(delete_tenant_policy),
        )
        .route("/metrics/summary", get(metrics_summary))
        .route("/metrics/ceph", get(metrics_ceph))
        .route("/metrics/history", get(metrics_history))
        .route("/metrics/forecast", get(metrics_forecast))
        .route("/alerts", get(list_alerts))
        .route("/alerts/evaluate", post(evaluate_alerts))
        .route("/jobs", get(list_jobs))
        .route("/jobs/{id}", get(get_job))
        .route("/jobs/{id}/watch", get(watch_job_sse))
        .route("/audit", get(list_audit))
        .route("/events", get(list_events))
        .route_layer(middleware::from_fn_with_state(
            state.clone(),
            auth_middleware,
        ))
        .with_state(state.clone());

    Router::new()
        .route("/health", get(health))
        .route("/readyz", get(readyz))
        .route("/version", get(version))
        .route("/metrics", get(prometheus_metrics))
        .nest("/api/atlas/v1", api)
        // Any other path serves the embedded Storage Center SPA (client-side routing).
        .fallback(get(spa_handler))
        .with_state(state)
}

/// `GET /metrics` — Atlas's own operational state in Prometheus text-exposition format, so a
/// Prometheus/Grafana stack can scrape the control plane itself (unauthenticated, like `/health`).
async fn prometheus_metrics(State(s): State<AppState>) -> impl axum::response::IntoResponse {
    use std::fmt::Write;
    let mut out = String::with_capacity(1024);
    let summary = atlas_inventory::metrics_summary(&s.pool)
        .await
        .unwrap_or(Value::Null);
    let g = |o: &mut String, name: &str, help: &str, v: i64| {
        let _ = writeln!(o, "# HELP {name} {help}\n# TYPE {name} gauge\n{name} {v}");
    };
    let n = |k: &str| summary.get(k).and_then(|x| x.as_i64()).unwrap_or(0);
    let _ = writeln!(
        out,
        "# HELP atlas_build_info Atlas gateway build info.\n# TYPE atlas_build_info gauge\natlas_build_info{{version=\"{}\"}} 1",
        env!("CARGO_PKG_VERSION")
    );
    g(&mut out, "atlas_volumes", "Total volumes.", n("volumes"));
    g(
        &mut out,
        "atlas_snapshots",
        "Total snapshots.",
        n("snapshots"),
    );
    g(&mut out, "atlas_buckets", "Total buckets.", n("buckets"));
    g(&mut out, "atlas_backups", "Total backups.", n("backups"));
    g(&mut out, "atlas_pools", "Total pools.", n("pools"));
    g(&mut out, "atlas_clusters", "Total clusters.", n("clusters"));
    g(
        &mut out,
        "atlas_capacity_raw_bytes",
        "Raw cluster capacity (bytes).",
        n("raw_capacity_bytes"),
    );
    g(
        &mut out,
        "atlas_capacity_used_bytes",
        "Used cluster capacity (bytes).",
        n("used_capacity_bytes"),
    );
    // Jobs by state.
    if let Ok(states) = atlas_inventory::jobs::count_by_state(&s.pool).await {
        let _ = writeln!(
            out,
            "# HELP atlas_jobs Total jobs by state.\n# TYPE atlas_jobs gauge"
        );
        for (state, count) in states {
            let _ = writeln!(out, "atlas_jobs{{state=\"{state}\"}} {count}");
        }
    }
    // Open alerts.
    if let Ok(alerts) = atlas_inventory::alerts::list(&s.pool, Some("open")).await {
        g(
            &mut out,
            "atlas_alerts_open",
            "Open alerts.",
            alerts.len() as i64,
        );
    }
    // Per-backend breakdown (labelled by backend id + type).
    if let Ok(backends) = atlas_inventory::backend_breakdown(&s.pool).await {
        g(
            &mut out,
            "atlas_backends",
            "Registered backends.",
            backends.len() as i64,
        );
        let bn = |b: &Value, k: &str| b.get(k).and_then(|x| x.as_i64()).unwrap_or(0);
        let bs = |b: &Value, k: &str| b.get(k).and_then(|x| x.as_str()).unwrap_or("").to_string();
        let _ = writeln!(
            out,
            "# HELP atlas_backend_volumes Volumes per backend.\n# TYPE atlas_backend_volumes gauge"
        );
        for b in &backends {
            let _ = writeln!(
                out,
                "atlas_backend_volumes{{backend=\"{}\",type=\"{}\"}} {}",
                bs(b, "backend_id"),
                bs(b, "backend_type"),
                bn(b, "volumes")
            );
        }
        let _ = writeln!(
            out,
            "# HELP atlas_backend_capacity_raw_bytes Raw capacity per backend.\n# TYPE atlas_backend_capacity_raw_bytes gauge"
        );
        for b in &backends {
            let _ = writeln!(
                out,
                "atlas_backend_capacity_raw_bytes{{backend=\"{}\",type=\"{}\"}} {}",
                bs(b, "backend_id"),
                bs(b, "backend_type"),
                bn(b, "raw_capacity_bytes")
            );
        }
    }
    (
        [(
            axum::http::header::CONTENT_TYPE,
            "text/plain; version=0.0.4",
        )],
        out,
    )
}

/// The compiled Storage Center SPA (`crates/atlas-gateway/ui/dist`), embedded into the binary.
#[derive(rust_embed::Embed)]
#[folder = "ui/dist"]
struct Ui;

/// Serve an embedded UI asset by path; fall back to `index.html` for client-side routes.
async fn spa_handler(uri: axum::http::Uri) -> axum::response::Response {
    use axum::response::IntoResponse;
    let path = uri.path().trim_start_matches('/');
    let path = if path.is_empty() { "index.html" } else { path };
    let (body, file) = match Ui::get(path) {
        Some(f) => (f.data, path.to_string()),
        None => match Ui::get("index.html") {
            Some(f) => (f.data, "index.html".to_string()),
            None => {
                return (
                    StatusCode::NOT_FOUND,
                    "UI not built — run `make ui` (or build the Docker image)",
                )
                    .into_response()
            }
        },
    };
    let mime = mime_guess::from_path(&file).first_or_octet_stream();
    (
        [(axum::http::header::CONTENT_TYPE, mime.as_ref())],
        body.into_owned(),
    )
        .into_response()
}

// ---- meta ----

async fn health() -> Json<Value> {
    Json(json!({ "status": "ok" }))
}

/// `GET /readyz` — readiness deep-check: probes the DB, confirms migrations/inventory are readable,
/// and reports driver + Kubernetes attachment. Returns 200 when ready, 503 when not (for k8s probes).
/// `/health` stays a cheap liveness signal (process is up); this checks dependencies.
async fn readyz(State(s): State<AppState>) -> (StatusCode, Json<Value>) {
    use atlas_common::config::CephDriverMode;

    // DB reachable + migrated (a readable backend row proves both).
    let (db_ok, db_detail) =
        match sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM storage_backends")
            .fetch_one(&s.pool)
            .await
        {
            Ok(n) => (true, format!("{n} backend(s)")),
            Err(e) => (false, format!("query failed: {e}")),
        };

    let driver_mode = match s.config.ceph_driver_mode {
        CephDriverMode::Real => "real",
        CephDriverMode::Fake => "fake",
    };
    let k8s_ok = s.k8s.is_some();

    // Readiness gates on the DB only; k8s/driver are reported but a fake-mode or k8s-less lab
    // is still "ready" to serve the read/inventory API.
    let ready = db_ok;
    let code = if ready {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    };
    (
        code,
        Json(json!({
            "status": if ready { "ready" } else { "not_ready" },
            "components": {
                "database": { "ok": db_ok, "detail": db_detail },
                "ceph_driver": { "ok": true, "mode": driver_mode },
                "kubernetes": { "ok": k8s_ok, "detail": if k8s_ok { "attached" } else { "not attached" } },
            },
        })),
    )
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

/// `GET /backends/summary` — per-backend inventory breakdown (type, clusters/volumes, capacity).
async fn backends_summary(State(s): State<AppState>) -> AppResult<Json<Value>> {
    Ok(Json(json!(
        atlas_inventory::backend_breakdown(&s.pool).await?
    )))
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

    let rbd_owners = s.rbd_owners().await;
    let result = atlas_discovery::run_discovery(&s.pool, driver, rbd_owners.as_ref()).await;
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

/// `GET /ceph/status` — live `ceph status` from the cluster (health, quorum, osdmap, pgmap, I/O).
async fn get_ceph_status(State(s): State<AppState>) -> AppResult<Json<Value>> {
    let d = s
        .driver_for(CEPH_BACKEND_ID)
        .ok_or_else(|| AppError::Driver("no ceph driver".into()))?;
    Ok(Json(
        d.ceph_status()
            .await
            .map_err(|e| AppError::Driver(e.to_string()))?,
    ))
}

/// `GET /ceph/osd-tree` — the CRUSH hierarchy (roots → hosts → OSDs).
async fn get_ceph_osd_tree(State(s): State<AppState>) -> AppResult<Json<Value>> {
    let d = s
        .driver_for(CEPH_BACKEND_ID)
        .ok_or_else(|| AppError::Driver("no ceph driver".into()))?;
    Ok(Json(
        d.ceph_osd_tree()
            .await
            .map_err(|e| AppError::Driver(e.to_string()))?,
    ))
}

/// `GET /ceph/df` — cluster + per-pool capacity/usage/objects (`ceph df detail`).
async fn get_ceph_df(State(s): State<AppState>) -> AppResult<Json<Value>> {
    let d = s
        .driver_for(CEPH_BACKEND_ID)
        .ok_or_else(|| AppError::Driver("no ceph driver".into()))?;
    Ok(Json(
        d.ceph_df()
            .await
            .map_err(|e| AppError::Driver(e.to_string()))?,
    ))
}

/// `GET /ceph/osd-df` — per-OSD utilization (`ceph osd df`).
async fn get_ceph_osd_df(State(s): State<AppState>) -> AppResult<Json<Value>> {
    let d = s
        .driver_for(CEPH_BACKEND_ID)
        .ok_or_else(|| AppError::Driver("no ceph driver".into()))?;
    Ok(Json(
        d.ceph_osd_df()
            .await
            .map_err(|e| AppError::Driver(e.to_string()))?,
    ))
}

#[derive(Debug, Deserialize)]
struct PoolQuery {
    backend: Option<String>,
    kind: Option<String>,
}

async fn list_pools(
    State(s): State<AppState>,
    Query(q): Query<PoolQuery>,
) -> AppResult<Json<Value>> {
    let pools = if q.backend.is_some() || q.kind.is_some() {
        atlas_inventory::list_pools_filtered(&s.pool, q.backend.as_deref(), q.kind.as_deref())
            .await?
    } else {
        atlas_inventory::list_pools(&s.pool).await?
    };
    Ok(Json(json!(pools)))
}

#[derive(Debug, Deserialize)]
struct VolumeQuery {
    state: Option<String>,
    tenant: Option<String>,
    backend: Option<String>,
    kind: Option<String>,
}

async fn list_volumes(
    State(s): State<AppState>,
    Query(q): Query<VolumeQuery>,
) -> AppResult<Json<Value>> {
    let vols = if q.state.is_some() || q.tenant.is_some() || q.backend.is_some() || q.kind.is_some()
    {
        atlas_inventory::list_volumes_filtered(
            &s.pool,
            q.state.as_deref(),
            q.tenant.as_deref(),
            q.backend.as_deref(),
            q.kind.as_deref(),
        )
        .await?
    } else {
        atlas_inventory::list_volumes(&s.pool).await?
    };
    Ok(Json(json!(vols)))
}

/// `GET /volumes.csv[?state=&tenant=&backend=&kind=]` — the volume inventory as a downloadable CSV
/// (honors the same filters as `/volumes`), for spreadsheets / capacity reporting.
async fn volumes_csv(
    State(s): State<AppState>,
    Query(q): Query<VolumeQuery>,
) -> impl axum::response::IntoResponse {
    use std::fmt::Write;
    let vols = if q.state.is_some() || q.tenant.is_some() || q.backend.is_some() || q.kind.is_some()
    {
        atlas_inventory::list_volumes_filtered(
            &s.pool,
            q.state.as_deref(),
            q.tenant.as_deref(),
            q.backend.as_deref(),
            q.kind.as_deref(),
        )
        .await
        .unwrap_or_default()
    } else {
        atlas_inventory::list_volumes(&s.pool)
            .await
            .unwrap_or_default()
    };

    let mut out = String::from(
        "id,name,kind,state,size_bytes,used_bytes,cluster_id,pool_id,namespace,pvc,storage_class\n",
    );
    let cell = |o: &mut String, v: &str, last: bool| {
        if v.contains([',', '"', '\n']) {
            let _ = write!(o, "\"{}\"", v.replace('"', "\"\""));
        } else {
            let _ = write!(o, "{v}");
        }
        o.push(if last { '\n' } else { ',' });
    };
    for vol in &vols {
        let val = serde_json::to_value(vol).unwrap_or(Value::Null);
        let g = |k: &str| match val.get(k) {
            Some(Value::String(s)) => s.clone(),
            Some(Value::Null) | None => String::new(),
            Some(other) => other.to_string(),
        };
        cell(&mut out, &g("id"), false);
        cell(&mut out, &g("name"), false);
        cell(&mut out, &g("kind"), false);
        cell(&mut out, &g("state"), false);
        cell(&mut out, &g("size_bytes"), false);
        cell(&mut out, &g("used_bytes"), false);
        cell(&mut out, &g("cluster_id"), false);
        cell(&mut out, &g("pool_id"), false);
        cell(&mut out, &g("kubernetes_namespace"), false);
        cell(&mut out, &g("pvc_name"), false);
        cell(&mut out, &g("storage_class_name"), true);
    }
    (
        [
            (axum::http::header::CONTENT_TYPE, "text/csv; charset=utf-8"),
            (
                axum::http::header::CONTENT_DISPOSITION,
                "attachment; filename=\"atlas-volumes.csv\"",
            ),
        ],
        out,
    )
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
struct HistoryQuery {
    minutes: Option<i64>,
}

/// `GET /metrics/history[?minutes=60]` — persisted capacity/IO/job time-series for trend charts.
async fn metrics_history(
    State(s): State<AppState>,
    Query(q): Query<HistoryQuery>,
) -> AppResult<Json<Value>> {
    let minutes = q.minutes.unwrap_or(60).clamp(1, 2880);
    Ok(Json(json!(
        atlas_inventory::metrics::history(&s.pool, minutes).await?
    )))
}

/// `GET /metrics/forecast[?minutes=1440]` — least-squares projection of days-until-full.
async fn metrics_forecast(
    State(s): State<AppState>,
    Query(q): Query<HistoryQuery>,
) -> AppResult<Json<Value>> {
    let minutes = q.minutes.unwrap_or(1440).clamp(1, 20160);
    Ok(Json(
        atlas_inventory::metrics::forecast(&s.pool, minutes).await?,
    ))
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

#[derive(Debug, Deserialize)]
struct ScheduleBody {
    /// Cadence in seconds.
    interval_secs: i64,
    /// Retain the newest N (0 = keep all).
    #[serde(default)]
    keep: i64,
    /// "snapshot" (default) or "backup".
    #[serde(default)]
    kind: Option<String>,
    /// Target bucket id (required for backup schedules).
    #[serde(default)]
    bucket_id: Option<String>,
    /// Backup mode ("manifest" default, or "data"); ignored for snapshot schedules.
    #[serde(default)]
    mode: Option<String>,
}

/// `POST /volumes/{id}/schedule` — create a protection schedule for a volume (operator).
async fn create_schedule(
    State(s): State<AppState>,
    Extension(actor): Extension<Actor>,
    Path(id): Path<String>,
    Json(body): Json<ScheduleBody>,
) -> AppResult<(StatusCode, Json<Value>)> {
    crate::auth::require_role(s.config.auth_required, &actor, crate::auth::ROLE_OPERATOR)?;
    if body.interval_secs <= 0 {
        return Err(AppError::Validation("interval_secs must be > 0".into()));
    }
    if body.keep < 0 {
        return Err(AppError::Validation("keep must be >= 0".into()));
    }
    let kind = body.kind.as_deref().unwrap_or("snapshot");
    if kind != "snapshot" && kind != "backup" {
        return Err(AppError::Validation(
            "kind must be snapshot or backup".into(),
        ));
    }
    atlas_inventory::get_volume(&s.pool, &id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("volume {id}")))?;
    let mode = body.mode.as_deref().unwrap_or("manifest");
    if kind == "backup" {
        let bucket_id = body
            .bucket_id
            .as_deref()
            .ok_or_else(|| AppError::Validation("backup schedule requires bucket_id".into()))?;
        let bucket = atlas_inventory::buckets::get_bucket(&s.pool, bucket_id)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("bucket {bucket_id}")))?;
        if bucket.state != "bound" {
            return Err(AppError::Validation(format!(
                "bucket {bucket_id} is not bound yet"
            )));
        }
    }
    let tenant_id = atlas_inventory::volume_tenant(&s.pool, &id).await?;
    let sched = atlas_inventory::schedules::insert(
        &s.pool,
        &ids::schedule_id(),
        &tenant_id,
        &id,
        kind,
        body.bucket_id.as_deref(),
        mode,
        body.interval_secs,
        body.keep,
    )
    .await?;
    Ok((StatusCode::CREATED, Json(json!(sched))))
}

/// `GET /schedules` (optionally `?volume_id=`).
async fn list_schedules(
    State(s): State<AppState>,
    Query(q): Query<ListBackupsQuery>,
) -> AppResult<Json<Value>> {
    let items = atlas_inventory::schedules::list(&s.pool, q.volume_id.as_deref()).await?;
    Ok(Json(json!(items)))
}

/// `DELETE /schedules/{id}` (operator).
async fn delete_schedule(
    State(s): State<AppState>,
    Extension(actor): Extension<Actor>,
    Path(id): Path<String>,
) -> AppResult<Json<Value>> {
    crate::auth::require_role(s.config.auth_required, &actor, crate::auth::ROLE_OPERATOR)?;
    let removed = atlas_inventory::schedules::delete(&s.pool, &id).await?;
    if !removed {
        return Err(AppError::NotFound(format!("schedule {id}")));
    }
    Ok(Json(json!({ "deleted": id })))
}

#[derive(Debug, Deserialize)]
struct IssueTokenBody {
    /// The token subject — typically the product/service-account name (goes into audit logs).
    subject: String,
    /// Role: `viewer`, `operator`, `admin`, or a `product.service.<name>` (→ operator). Default viewer.
    #[serde(default)]
    role: Option<String>,
    /// Lifetime in seconds (default 3600, capped at 90 days).
    #[serde(default)]
    ttl_secs: Option<u64>,
}

/// `POST /auth/tokens` — mint a scoped service-account JWT for a product (admin). The shared secret
/// never leaves Atlas; the caller receives only the signed token + its claims.
async fn issue_token(
    State(s): State<AppState>,
    Extension(actor): Extension<Actor>,
    Json(body): Json<IssueTokenBody>,
) -> AppResult<(StatusCode, Json<Value>)> {
    crate::auth::require_role(s.config.auth_required, &actor, crate::auth::ROLE_ADMIN)?;
    if body.subject.trim().is_empty() {
        return Err(AppError::Validation("subject is required".into()));
    }
    let role = body.role.unwrap_or_else(|| "viewer".into());
    // Cap the lifetime so a leaked token has a bounded blast radius.
    const MAX_TTL: u64 = 90 * 24 * 3600;
    let ttl_secs = body.ttl_secs.unwrap_or(3600).clamp(60, MAX_TTL);
    let (token, exp) =
        crate::auth::mint_token(&s.config.jwt_secret, &body.subject, &role, ttl_secs)?;
    let _ = atlas_inventory::audit::record(
        &s.pool,
        None,
        &actor.id,
        "auth.token.issued",
        "service_account",
        &body.subject,
        "ok",
        Some(json!({ "role": role, "ttl_secs": ttl_secs })),
        None,
    )
    .await;
    Ok((
        StatusCode::CREATED,
        Json(json!({
            "token": token, "subject": body.subject, "role": role,
            "level": crate::auth::role_level(&role), "expires_at": exp, "ttl_secs": ttl_secs
        })),
    ))
}

#[derive(Debug, Deserialize)]
struct AuditQuery {
    actor: Option<String>,
    action: Option<String>,
    resource_type: Option<String>,
    resource_id: Option<String>,
    limit: Option<i64>,
}

const DEFAULT_RBD_POOL: &str = "rbd-nvme-prod";

#[derive(Debug, Deserialize)]
struct CreateRbdBody {
    /// Image name.
    name: String,
    size_bytes: i64,
    /// RBD pool (defaults to the platform block pool).
    #[serde(default)]
    pool: Option<String>,
    #[serde(default)]
    tenant_id: Option<String>,
}

/// `POST /rbd-usage/refresh` — recompute `used_bytes` for every RBD-backed volume via `rbd du`
/// (operator). Resolves each volume's image (direct `rbd:` id, or the PVC's PV for CSI volumes).
async fn refresh_rbd_usage(
    State(s): State<AppState>,
    Extension(actor): Extension<Actor>,
) -> AppResult<Json<Value>> {
    crate::auth::require_role(s.config.auth_required, &actor, crate::auth::ROLE_OPERATOR)?;
    let volumes = atlas_inventory::list_volumes(&s.pool).await?;
    let mut updated = 0usize;
    let mut total_used: i64 = 0;
    for v in volumes {
        // Determine the (pool, image) for this volume.
        let target = if let Some(native) = v
            .backend_native_id
            .as_deref()
            .and_then(|n| n.strip_prefix("rbd:"))
        {
            native
                .split_once('/')
                .map(|(p, i)| (p.to_string(), i.to_string()))
        } else if let (Some(ns), Some(pvc)) = (&v.kubernetes_namespace, &v.pvc_name) {
            match &s.k8s {
                Some(k8s) => k8s.resolve_rbd(ns, pvc).await.ok().flatten(),
                None => None,
            }
        } else {
            None
        };
        let Some((rbd_pool, image)) = target else {
            continue;
        };
        if let Ok(used) = atlas_driver_ceph::rbd_du_image(&rbd_pool, &image).await {
            if atlas_inventory::set_volume_used(&s.pool, &v.id, used)
                .await
                .is_ok()
            {
                updated += 1;
                total_used += used;
            }
        }
    }
    Ok(Json(
        json!({ "updated": updated, "total_used_bytes": total_used }),
    ))
}

/// `POST /rbd-images` — provision a raw RBD image directly (bypassing CSI) for non-K8s consumers.
async fn create_rbd_image(
    State(s): State<AppState>,
    Extension(actor): Extension<Actor>,
    Json(body): Json<CreateRbdBody>,
) -> AppResult<(StatusCode, Json<Value>)> {
    crate::auth::require_role(s.config.auth_required, &actor, crate::auth::ROLE_OPERATOR)?;
    if body.name.trim().is_empty() {
        return Err(AppError::Validation("name is required".into()));
    }
    if body.size_bytes <= 0 {
        return Err(AppError::Validation("size_bytes must be > 0".into()));
    }
    let pool_name = body.pool.unwrap_or_else(|| DEFAULT_RBD_POOL.into());
    let tenant_id = body.tenant_id.unwrap_or_else(|| "global".into());
    let volume_id = ids::volume_id();
    let job_id = ids::job_id();
    let spec = JobSpec::RbdCreate {
        volume_id: volume_id.clone(),
        backend_id: CEPH_BACKEND_ID.into(),
        pool: pool_name.clone(),
        image: body.name.clone(),
        size_bytes: body.size_bytes,
    };
    let job = s
        .jobs
        .enqueue(&job_id, &tenant_id, &actor.id, spec, None)
        .await
        .map_err(AppError::from)?;
    Ok(accepted(
        &job,
        json!({ "volume_id": volume_id, "rbd": format!("{pool_name}/{}", body.name) }),
    ))
}

/// `DELETE /rbd-images/{pool}/{image}` — delete a raw RBD image (admin).
async fn delete_rbd_image(
    State(s): State<AppState>,
    Extension(actor): Extension<Actor>,
    Path((pool_name, image)): Path<(String, String)>,
) -> AppResult<(StatusCode, Json<Value>)> {
    crate::auth::require_role(s.config.auth_required, &actor, crate::auth::ROLE_ADMIN)?;
    // Best-effort: resolve the recorded volume row (by native id) so we clean up inventory too.
    let native = format!("rbd:{pool_name}/{image}");
    let volume_id = atlas_inventory::list_volumes(&s.pool)
        .await?
        .into_iter()
        .find(|v| v.backend_native_id.as_deref() == Some(native.as_str()))
        .map(|v| v.id)
        .unwrap_or_default();
    let job_id = ids::job_id();
    let spec = JobSpec::RbdDelete {
        volume_id,
        pool: pool_name.clone(),
        image: image.clone(),
    };
    let job = s
        .jobs
        .enqueue(&job_id, "global", &actor.id, spec, None)
        .await
        .map_err(AppError::from)?;
    Ok(accepted(
        &job,
        json!({ "rbd": format!("{pool_name}/{image}") }),
    ))
}

#[derive(Debug, Deserialize)]
struct CloneRbdBody {
    /// New clone image name.
    name: String,
    /// Snapshot name to create + protect on the parent (defaults to `<clone>-base`).
    #[serde(default)]
    snap: Option<String>,
    #[serde(default)]
    tenant_id: Option<String>,
}

/// `POST /rbd-images/{pool}/{image}/clone` — snapshot+protect the parent and create a COW clone.
async fn clone_rbd_image(
    State(s): State<AppState>,
    Extension(actor): Extension<Actor>,
    Path((pool_name, image)): Path<(String, String)>,
    Json(body): Json<CloneRbdBody>,
) -> AppResult<(StatusCode, Json<Value>)> {
    crate::auth::require_role(s.config.auth_required, &actor, crate::auth::ROLE_OPERATOR)?;
    if body.name.trim().is_empty() {
        return Err(AppError::Validation("name is required".into()));
    }
    let snap = body.snap.unwrap_or_else(|| format!("{}-base", body.name));
    let tenant_id = body.tenant_id.unwrap_or_else(|| "global".into());
    let volume_id = ids::volume_id();
    let job_id = ids::job_id();
    let spec = JobSpec::RbdClone {
        volume_id: volume_id.clone(),
        backend_id: CEPH_BACKEND_ID.into(),
        pool: pool_name.clone(),
        image: image.clone(),
        snap: snap.clone(),
        clone_image: body.name.clone(),
    };
    let job = s
        .jobs
        .enqueue(&job_id, &tenant_id, &actor.id, spec, None)
        .await
        .map_err(AppError::from)?;
    Ok(accepted(
        &job,
        json!({ "volume_id": volume_id, "clone": format!("{pool_name}/{}", body.name),
                "parent": format!("{pool_name}/{image}@{snap}") }),
    ))
}

#[derive(Debug, Deserialize)]
struct ResizeRbdBody {
    size_bytes: i64,
}

/// `POST /rbd-images/{pool}/{image}/resize` — grow a raw RBD image (operator).
async fn resize_rbd_image(
    State(s): State<AppState>,
    Extension(actor): Extension<Actor>,
    Path((pool_name, image)): Path<(String, String)>,
    Json(body): Json<ResizeRbdBody>,
) -> AppResult<(StatusCode, Json<Value>)> {
    crate::auth::require_role(s.config.auth_required, &actor, crate::auth::ROLE_OPERATOR)?;
    if body.size_bytes <= 0 {
        return Err(AppError::Validation("size_bytes must be > 0".into()));
    }
    let native = format!("rbd:{pool_name}/{image}");
    let volume_id = atlas_inventory::list_volumes(&s.pool)
        .await?
        .into_iter()
        .find(|v| v.backend_native_id.as_deref() == Some(native.as_str()))
        .map(|v| v.id)
        .unwrap_or_default();
    let job_id = ids::job_id();
    let spec = JobSpec::RbdResize {
        volume_id,
        pool: pool_name.clone(),
        image: image.clone(),
        new_size_bytes: body.size_bytes,
    };
    let job = s
        .jobs
        .enqueue(&job_id, "global", &actor.id, spec, None)
        .await
        .map_err(AppError::from)?;
    Ok(accepted(
        &job,
        json!({ "rbd": format!("{pool_name}/{image}"), "new_size_bytes": body.size_bytes }),
    ))
}

/// `POST /rbd-images/{pool}/{image}/flatten` — detach a COW clone from its parent (operator).
async fn flatten_rbd_image(
    State(s): State<AppState>,
    Extension(actor): Extension<Actor>,
    Path((pool_name, image)): Path<(String, String)>,
) -> AppResult<(StatusCode, Json<Value>)> {
    crate::auth::require_role(s.config.auth_required, &actor, crate::auth::ROLE_OPERATOR)?;
    let job_id = ids::job_id();
    let spec = JobSpec::RbdFlatten {
        pool: pool_name.clone(),
        image: image.clone(),
    };
    let job = s
        .jobs
        .enqueue(&job_id, "global", &actor.id, spec, None)
        .await
        .map_err(AppError::from)?;
    Ok(accepted(
        &job,
        json!({ "rbd": format!("{pool_name}/{image}") }),
    ))
}

/// `GET /rbd-images/{pool}/{image}/snapshots` — list a raw image's snapshots (from Ceph).
async fn list_rbd_snaps(
    State(s): State<AppState>,
    Path((pool_name, image)): Path<(String, String)>,
) -> AppResult<Json<Value>> {
    let _ = &s;
    let snaps = atlas_driver_ceph::rbd_snap_list(&pool_name, &image)
        .await
        .map_err(|e| AppError::Driver(e.to_string()))?;
    Ok(Json(
        json!({ "rbd": format!("{pool_name}/{image}"), "snapshots": snaps }),
    ))
}

#[derive(Debug, Deserialize)]
struct RbdSnapBody {
    name: String,
}

/// `POST /rbd-images/{pool}/{image}/snapshots` — snapshot a raw RBD image (operator).
async fn create_rbd_snap(
    State(s): State<AppState>,
    Extension(actor): Extension<Actor>,
    Path((pool_name, image)): Path<(String, String)>,
    Json(body): Json<RbdSnapBody>,
) -> AppResult<(StatusCode, Json<Value>)> {
    crate::auth::require_role(s.config.auth_required, &actor, crate::auth::ROLE_OPERATOR)?;
    if body.name.trim().is_empty() {
        return Err(AppError::Validation("name is required".into()));
    }
    let job_id = ids::job_id();
    let spec = JobSpec::RbdSnapshot {
        pool: pool_name.clone(),
        image: image.clone(),
        snap: body.name.clone(),
    };
    let job = s
        .jobs
        .enqueue(&job_id, "global", &actor.id, spec, None)
        .await
        .map_err(AppError::from)?;
    Ok(accepted(
        &job,
        json!({ "snapshot": format!("{pool_name}/{image}@{}", body.name) }),
    ))
}

/// `POST /rbd-images/{pool}/{image}/rollback` — roll a raw image back to a snapshot (admin).
async fn rollback_rbd_image(
    State(s): State<AppState>,
    Extension(actor): Extension<Actor>,
    Path((pool_name, image)): Path<(String, String)>,
    Json(body): Json<RbdSnapBody>,
) -> AppResult<(StatusCode, Json<Value>)> {
    crate::auth::require_role(s.config.auth_required, &actor, crate::auth::ROLE_ADMIN)?;
    let job_id = ids::job_id();
    let spec = JobSpec::RbdRollback {
        pool: pool_name.clone(),
        image: image.clone(),
        snap: body.name.clone(),
    };
    let job = s
        .jobs
        .enqueue(&job_id, "global", &actor.id, spec, None)
        .await
        .map_err(AppError::from)?;
    Ok(accepted(
        &job,
        json!({ "rbd": format!("{pool_name}/{image}"), "rollback_to": body.name }),
    ))
}

#[derive(Debug, Deserialize)]
struct RbdListQuery {
    pool: Option<String>,
}

/// `GET /rbd-images?pool=` — list RBD image names in a pool, straight from Ceph.
async fn list_rbd_images(
    State(s): State<AppState>,
    Query(q): Query<RbdListQuery>,
) -> AppResult<Json<Value>> {
    let pool_name = q.pool.unwrap_or_else(|| DEFAULT_RBD_POOL.into());
    let driver = s
        .driver_for(CEPH_BACKEND_ID)
        .ok_or_else(|| AppError::Driver("no ceph driver".into()))?;
    // The image list comes from the live cluster; the fake driver has no RBD CLI, so guard on real.
    let _ = driver;
    let images = atlas_driver_ceph::rbd_list(&pool_name)
        .await
        .map_err(|e| AppError::Driver(e.to_string()))?;
    Ok(Json(json!({ "pool": pool_name, "images": images })))
}

/// `GET /volumes/{id}/bindings` — product ownership records for a volume.
async fn list_volume_bindings(
    State(s): State<AppState>,
    Path(id): Path<String>,
) -> AppResult<Json<Value>> {
    let rows = atlas_inventory::list_bindings_for(&s.pool, "volume", &id).await?;
    Ok(Json(json!(rows)))
}

/// `GET /volumes/{id}/labels` — the volume's user labels.
async fn get_volume_labels(
    State(s): State<AppState>,
    Path(id): Path<String>,
) -> AppResult<Json<Value>> {
    Ok(Json(
        atlas_inventory::get_volume_labels(&s.pool, &id).await?,
    ))
}

/// `PUT /volumes/{id}/labels` — merge labels into the volume (operator). Body is a JSON object.
async fn put_volume_labels(
    State(s): State<AppState>,
    Extension(actor): Extension<Actor>,
    Path(id): Path<String>,
    Json(body): Json<Value>,
) -> AppResult<Json<Value>> {
    crate::auth::require_role(s.config.auth_required, &actor, crate::auth::ROLE_OPERATOR)?;
    let labels = body
        .as_object()
        .ok_or_else(|| AppError::Validation("body must be a JSON object of labels".into()))?;
    atlas_inventory::get_volume(&s.pool, &id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("volume {id}")))?;
    let merged = atlas_inventory::set_volume_labels(&s.pool, &id, labels).await?;
    Ok(Json(merged))
}

/// `GET /tenants` — overview of every tenant with volumes or a quota (usage + limits).
async fn list_tenants(State(s): State<AppState>) -> AppResult<Json<Value>> {
    Ok(Json(json!(
        atlas_inventory::tenants::list_overview(&s.pool).await?
    )))
}

/// `GET /audit` — query the audit trail (operator), newest first. Filters: `actor`, `action`,
/// `resource_type`, `resource_id`; `limit` (default 100, max 1000).
async fn list_audit(
    State(s): State<AppState>,
    Extension(actor): Extension<Actor>,
    Query(q): Query<AuditQuery>,
) -> AppResult<Json<Value>> {
    crate::auth::require_role(s.config.auth_required, &actor, crate::auth::ROLE_OPERATOR)?;
    let rows = atlas_inventory::audit::list(
        &s.pool,
        q.actor.as_deref(),
        q.action.as_deref(),
        q.resource_type.as_deref(),
        q.resource_id.as_deref(),
        q.limit.unwrap_or(100),
    )
    .await?;
    Ok(Json(json!(rows)))
}

#[derive(Debug, Deserialize)]
struct EventsQuery {
    limit: Option<i64>,
}

/// `GET /events[?limit=100]` — unified activity feed (jobs + audit + alerts), newest first.
/// Operator-gated because it surfaces audit records.
async fn list_events(
    State(s): State<AppState>,
    Extension(actor): Extension<Actor>,
    Query(q): Query<EventsQuery>,
) -> AppResult<Json<Value>> {
    crate::auth::require_role(s.config.auth_required, &actor, crate::auth::ROLE_OPERATOR)?;
    let rows = atlas_inventory::events::feed(&s.pool, q.limit.unwrap_or(100)).await?;
    Ok(Json(json!(rows)))
}

/// `GET /tenants/{id}/policies` — the tenant's intent→placement overrides.
async fn list_tenant_policies(
    State(s): State<AppState>,
    Path(id): Path<String>,
) -> AppResult<Json<Value>> {
    let items = atlas_inventory::tenants::list_policies(&s.pool, &id).await?;
    Ok(Json(json!(items)))
}

#[derive(Debug, Deserialize)]
struct TenantPolicyBody {
    storage_class: String,
    #[serde(default)]
    access_mode: Option<String>,
    #[serde(default)]
    volume_mode: Option<String>,
}

/// `PUT /tenants/{id}/policies/{intent}` — override an intent's placement for a tenant (admin).
async fn put_tenant_policy(
    State(s): State<AppState>,
    Extension(actor): Extension<Actor>,
    Path((id, intent)): Path<(String, String)>,
    Json(body): Json<TenantPolicyBody>,
) -> AppResult<Json<Value>> {
    crate::auth::require_role(s.config.auth_required, &actor, crate::auth::ROLE_ADMIN)?;
    if body.storage_class.trim().is_empty() {
        return Err(AppError::Validation("storage_class is required".into()));
    }
    let access_mode = body.access_mode.unwrap_or_else(|| "ReadWriteOnce".into());
    let volume_mode = body.volume_mode.unwrap_or_else(|| "Filesystem".into());
    atlas_inventory::tenants::set_policy(
        &s.pool,
        &id,
        &intent,
        &body.storage_class,
        &access_mode,
        &volume_mode,
    )
    .await?;
    let _ = atlas_inventory::audit::record(
        &s.pool,
        None,
        &actor.id,
        "tenant.policy.set",
        "tenant",
        &id,
        "ok",
        Some(json!({ "intent": intent, "storage_class": body.storage_class })),
        None,
    )
    .await;
    let p = atlas_inventory::tenants::get_policy(&s.pool, &id, &intent).await?;
    Ok(Json(json!(p)))
}

/// `DELETE /tenants/{id}/policies/{intent}` (admin).
async fn delete_tenant_policy(
    State(s): State<AppState>,
    Extension(actor): Extension<Actor>,
    Path((id, intent)): Path<(String, String)>,
) -> AppResult<Json<Value>> {
    crate::auth::require_role(s.config.auth_required, &actor, crate::auth::ROLE_ADMIN)?;
    let removed = atlas_inventory::tenants::delete_policy(&s.pool, &id, &intent).await?;
    if !removed {
        return Err(AppError::NotFound(format!(
            "tenant {id} has no override for intent {intent}"
        )));
    }
    Ok(Json(
        json!({ "deleted": { "tenant_id": id, "intent": intent } }),
    ))
}

/// `GET /tenants/{id}/quota` — the tenant's quota + current usage (unlimited 0/0 if unset).
async fn get_tenant_quota(
    State(s): State<AppState>,
    Path(id): Path<String>,
) -> AppResult<Json<Value>> {
    let q = atlas_inventory::tenants::get_quota(&s.pool, &id).await?;
    Ok(Json(json!(q)))
}

#[derive(Debug, Deserialize)]
struct TenantQuotaBody {
    /// Max total provisioned volume bytes (0 = unlimited).
    #[serde(default)]
    max_bytes: i64,
    /// Max number of volumes (0 = unlimited).
    #[serde(default)]
    max_volumes: i64,
}

/// `PUT /tenants/{id}/quota` — set the tenant's quota (admin).
async fn put_tenant_quota(
    State(s): State<AppState>,
    Extension(actor): Extension<Actor>,
    Path(id): Path<String>,
    Json(body): Json<TenantQuotaBody>,
) -> AppResult<Json<Value>> {
    crate::auth::require_role(s.config.auth_required, &actor, crate::auth::ROLE_ADMIN)?;
    if body.max_bytes < 0 || body.max_volumes < 0 {
        return Err(AppError::Validation("quota limits must be >= 0".into()));
    }
    atlas_inventory::tenants::set_quota(&s.pool, &id, body.max_bytes, body.max_volumes).await?;
    let _ = atlas_inventory::audit::record(
        &s.pool,
        None,
        &actor.id,
        "tenant.quota.set",
        "tenant",
        &id,
        "ok",
        Some(json!({ "max_bytes": body.max_bytes, "max_volumes": body.max_volumes })),
        None,
    )
    .await;
    let q = atlas_inventory::tenants::get_quota(&s.pool, &id).await?;
    Ok(Json(json!(q)))
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

    // Tenant quota admission (PDF §14): reject a create that would exceed the tenant's limits.
    match atlas_inventory::tenants::check_admission(&s.pool, &body.tenant_id, body.size_bytes)
        .await?
    {
        atlas_inventory::tenants::QuotaCheck::Ok => {}
        atlas_inventory::tenants::QuotaCheck::Bytes { limit, would_be } => {
            return Err(AppError::Conflict(format!(
                "tenant {} byte quota exceeded: {would_be} > {limit}",
                body.tenant_id
            )));
        }
        atlas_inventory::tenants::QuotaCheck::Count { limit, current } => {
            return Err(AppError::Conflict(format!(
                "tenant {} volume-count quota exceeded: {current} already at limit {limit}",
                body.tenant_id
            )));
        }
    }

    let k8s_opts = body.kubernetes.clone();
    let namespace = k8s_opts
        .as_ref()
        .and_then(|k| k.namespace.clone())
        .unwrap_or_else(|| "default".into());
    let sc_override = k8s_opts.as_ref().and_then(|k| k.storage_class.clone());
    let mut placement =
        atlas_policy::resolve(body.policy.as_deref(), body.kind, sc_override.as_deref());
    // Per-tenant policy override (PDF §14): unless the request pins an explicit StorageClass, a
    // tenant's override for this intent wins over the built-in catalog.
    if sc_override.is_none() {
        if let Some(intent) = body.policy.as_deref() {
            if let Some(tp) =
                atlas_inventory::tenants::get_policy(&s.pool, &body.tenant_id, intent).await?
            {
                placement.storage_class = tp.storage_class;
                placement.access_mode = tp.access_mode;
                placement.volume_mode = tp.volume_mode;
            }
        }
    }
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

// ---- DataBridge: sources (cloud-to-edge DB migration) ----

#[derive(Debug, Deserialize)]
struct CreateSourceBody {
    name: String,
    /// postgres | mysql | mariadb | oracle | sqlserver | mongodb
    kind: String,
    /// rds | aurora | cloudsql | generic
    #[serde(default)]
    cloud: Option<String>,
    endpoint: Option<String>,
    port: Option<i64>,
    database: Option<String>,
    /// k8s Secret with the source credentials (used only in real driver mode).
    secret_ref: Option<String>,
    secret_namespace: Option<String>,
    tls_mode: Option<String>,
    /// fake (default) | real
    driver_mode: Option<String>,
}

async fn db_list_sources(State(s): State<AppState>) -> AppResult<Json<Value>> {
    Ok(Json(json!(
        atlas_inventory::databridge::sources::list_sources(&s.pool).await?
    )))
}

async fn db_get_source(State(s): State<AppState>, Path(id): Path<String>) -> AppResult<Json<Value>> {
    let src = atlas_inventory::databridge::sources::get_source(&s.pool, &id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("source {id}")))?;
    Ok(Json(json!(src)))
}

/// `POST /databridge/sources` — register a cloud/source database (synchronous; no job).
async fn db_create_source(
    State(s): State<AppState>,
    Extension(actor): Extension<Actor>,
    Json(body): Json<CreateSourceBody>,
) -> AppResult<(StatusCode, Json<Value>)> {
    crate::auth::require_role(s.config.auth_required, &actor, crate::auth::ROLE_OPERATOR)?;
    if body.name.trim().is_empty() {
        return Err(AppError::Validation("name is required".into()));
    }
    if atlas_databridge::SourceKind::parse(&body.kind).is_none() {
        return Err(AppError::Validation(
            "kind must be one of: postgres, mysql, mariadb, oracle, sqlserver, mongodb".into(),
        ));
    }
    let source_id = ids::source_id();
    let cloud = body.cloud.as_deref().unwrap_or("generic");
    let tls_mode = body.tls_mode.as_deref().unwrap_or("require");
    let driver_mode = body.driver_mode.as_deref().unwrap_or("fake");
    atlas_inventory::databridge::sources::insert_source(
        &s.pool,
        &source_id,
        "global",
        &body.name,
        &body.kind,
        cloud,
        body.endpoint.as_deref(),
        body.port,
        body.database.as_deref(),
        body.secret_ref.as_deref(),
        body.secret_namespace.as_deref(),
        tls_mode,
        driver_mode,
    )
    .await?;
    let _ = atlas_inventory::audit::record(
        &s.pool,
        None,
        &actor.id,
        "databridge.source.create",
        "migration.source",
        &source_id,
        "success",
        Some(json!({ "name": body.name, "kind": body.kind, "cloud": cloud })),
        None,
    )
    .await;
    let src = atlas_inventory::databridge::sources::get_source(&s.pool, &source_id)
        .await?
        .ok_or_else(|| AppError::Internal("source vanished after insert".into()))?;
    Ok((StatusCode::CREATED, Json(json!(src))))
}

async fn db_delete_source(
    State(s): State<AppState>,
    Extension(actor): Extension<Actor>,
    Path(id): Path<String>,
) -> AppResult<Json<Value>> {
    crate::auth::require_role(s.config.auth_required, &actor, crate::auth::ROLE_OPERATOR)?;
    atlas_inventory::databridge::sources::delete_source_row(&s.pool, &id).await?;
    Ok(Json(json!({ "source_id": id, "deleted": true })))
}

/// `POST /databridge/sources/{id}/discover` — discover the source's schema as an async job.
async fn db_discover_source(
    State(s): State<AppState>,
    Extension(actor): Extension<Actor>,
    Path(id): Path<String>,
) -> AppResult<(StatusCode, Json<Value>)> {
    crate::auth::require_role(s.config.auth_required, &actor, crate::auth::ROLE_OPERATOR)?;
    atlas_inventory::databridge::sources::get_source(&s.pool, &id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("source {id}")))?;
    let job_id = ids::job_id();
    let spec = JobSpec::SourceDiscover {
        source_id: id.clone(),
    };
    let job = s
        .jobs
        .enqueue(&job_id, "global", &actor.id, spec, None)
        .await
        .map_err(AppError::from)?;
    Ok(accepted(&job, json!({ "source_id": id })))
}

// ---- DataBridge: migration plans ----

#[derive(Debug, Deserialize)]
struct CreatePlanBody {
    name: String,
    source_id: String,
    /// Rollback window after cutover, in seconds (default 72h).
    rollback_window_secs: Option<i64>,
}

async fn db_list_plans(State(s): State<AppState>) -> AppResult<Json<Value>> {
    Ok(Json(json!(
        atlas_inventory::databridge::plans::list_plans(&s.pool).await?
    )))
}

async fn db_get_plan(State(s): State<AppState>, Path(id): Path<String>) -> AppResult<Json<Value>> {
    let p = atlas_inventory::databridge::plans::get_plan(&s.pool, &id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("plan {id}")))?;
    Ok(Json(json!(p)))
}

/// `POST /databridge/plans` — create a migration plan for a source (synchronous).
async fn db_create_plan(
    State(s): State<AppState>,
    Extension(actor): Extension<Actor>,
    Json(body): Json<CreatePlanBody>,
) -> AppResult<(StatusCode, Json<Value>)> {
    crate::auth::require_role(s.config.auth_required, &actor, crate::auth::ROLE_OPERATOR)?;
    if body.name.trim().is_empty() {
        return Err(AppError::Validation("name is required".into()));
    }
    let source = atlas_inventory::databridge::sources::get_source(&s.pool, &body.source_id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("source {}", body.source_id)))?;
    let plan_id = ids::migration_plan_id();
    let window = body.rollback_window_secs.unwrap_or(259_200);
    atlas_inventory::databridge::plans::insert_plan(
        &s.pool, &plan_id, "global", &body.name, &source.id, window,
    )
    .await?;
    // If the source is already discovered, reflect that in the plan state.
    if source.state == "discovered" {
        atlas_inventory::databridge::plans::set_state(&s.pool, &plan_id, "discovered").await?;
    }
    let _ = atlas_inventory::audit::record(
        &s.pool,
        None,
        &actor.id,
        "databridge.plan.create",
        "migration.plan",
        &plan_id,
        "success",
        Some(json!({ "name": body.name, "source_id": source.id })),
        None,
    )
    .await;
    let p = atlas_inventory::databridge::plans::get_plan(&s.pool, &plan_id)
        .await?
        .ok_or_else(|| AppError::Internal("plan vanished after insert".into()))?;
    Ok((StatusCode::CREATED, Json(json!(p))))
}

async fn db_delete_plan(
    State(s): State<AppState>,
    Extension(actor): Extension<Actor>,
    Path(id): Path<String>,
) -> AppResult<Json<Value>> {
    crate::auth::require_role(s.config.auth_required, &actor, crate::auth::ROLE_OPERATOR)?;
    atlas_inventory::databridge::plans::delete_plan_row(&s.pool, &id).await?;
    Ok(Json(json!({ "plan_id": id, "deleted": true })))
}

/// `POST /databridge/plans/{id}/assess` — score readiness from the source's discovered schema (async).
async fn db_assess_plan(
    State(s): State<AppState>,
    Extension(actor): Extension<Actor>,
    Path(id): Path<String>,
) -> AppResult<(StatusCode, Json<Value>)> {
    crate::auth::require_role(s.config.auth_required, &actor, crate::auth::ROLE_OPERATOR)?;
    let plan = atlas_inventory::databridge::plans::get_plan(&s.pool, &id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("plan {id}")))?;
    let source = atlas_inventory::databridge::sources::get_source(&s.pool, &plan.source_id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("source {}", plan.source_id)))?;
    if source.state != "discovered" {
        return Err(AppError::Validation(format!(
            "source {} must be discovered before assessing (state: {})",
            source.id, source.state
        )));
    }
    let job_id = ids::job_id();
    let spec = JobSpec::MigrationAssess { plan_id: id.clone() };
    let job = s
        .jobs
        .enqueue(&job_id, "global", &actor.id, spec, None)
        .await
        .map_err(AppError::from)?;
    Ok(accepted(&job, json!({ "plan_id": id })))
}

/// `POST /databridge/plans/{id}/provision` — provision the edge DB cluster on Ceph (async).
async fn db_provision_edge(
    State(s): State<AppState>,
    Extension(actor): Extension<Actor>,
    Path(id): Path<String>,
) -> AppResult<(StatusCode, Json<Value>)> {
    crate::auth::require_role(s.config.auth_required, &actor, crate::auth::ROLE_OPERATOR)?;
    let plan = atlas_inventory::databridge::plans::get_plan(&s.pool, &id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("plan {id}")))?;
    if plan.readiness_score == 0 && plan.state == "draft" {
        return Err(AppError::Validation(
            "assess the plan before provisioning".into(),
        ));
    }
    let job_id = ids::job_id();
    let spec = JobSpec::EdgeDbProvision { plan_id: id.clone() };
    let job = s
        .jobs
        .enqueue(&job_id, "global", &actor.id, spec, None)
        .await
        .map_err(AppError::from)?;
    Ok(accepted(&job, json!({ "plan_id": id })))
}

async fn db_list_edge_clusters(State(s): State<AppState>) -> AppResult<Json<Value>> {
    Ok(Json(json!(
        atlas_inventory::databridge::edge_clusters::list_edge_clusters(&s.pool).await?
    )))
}

async fn db_get_edge_cluster(
    State(s): State<AppState>,
    Path(id): Path<String>,
) -> AppResult<Json<Value>> {
    let c = atlas_inventory::databridge::edge_clusters::get_edge_cluster(&s.pool, &id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("edge cluster {id}")))?;
    Ok(Json(json!(c)))
}

/// Cutover is refused unless the plan is validated + last validation passed + CDC lag is under this.
const CUTOVER_MAX_LAG_SECS: i64 = 10;

/// Enqueue a plan-scoped DataBridge stage job (operator role). Shared by the simple stage triggers.
async fn db_stage_job(
    s: &AppState,
    actor: &Actor,
    plan_id: &str,
    spec: JobSpec,
) -> AppResult<(StatusCode, Json<Value>)> {
    crate::auth::require_role(s.config.auth_required, actor, crate::auth::ROLE_OPERATOR)?;
    atlas_inventory::databridge::plans::get_plan(&s.pool, plan_id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("plan {plan_id}")))?;
    let job_id = ids::job_id();
    let job = s
        .jobs
        .enqueue(&job_id, "global", &actor.id, spec, None)
        .await
        .map_err(AppError::from)?;
    Ok(accepted(&job, json!({ "plan_id": plan_id })))
}

async fn db_full_load(
    State(s): State<AppState>,
    Extension(actor): Extension<Actor>,
    Path(id): Path<String>,
) -> AppResult<(StatusCode, Json<Value>)> {
    db_stage_job(&s, &actor, &id, JobSpec::FullLoad { plan_id: id.clone() }).await
}

async fn db_cdc_start(
    State(s): State<AppState>,
    Extension(actor): Extension<Actor>,
    Path(id): Path<String>,
) -> AppResult<(StatusCode, Json<Value>)> {
    db_stage_job(&s, &actor, &id, JobSpec::CdcStart { plan_id: id.clone() }).await
}

async fn db_cdc_stop(
    State(s): State<AppState>,
    Extension(actor): Extension<Actor>,
    Path(id): Path<String>,
) -> AppResult<(StatusCode, Json<Value>)> {
    db_stage_job(&s, &actor, &id, JobSpec::CdcStop { plan_id: id.clone() }).await
}

async fn db_validate(
    State(s): State<AppState>,
    Extension(actor): Extension<Actor>,
    Path(id): Path<String>,
) -> AppResult<(StatusCode, Json<Value>)> {
    db_stage_job(
        &s,
        &actor,
        &id,
        JobSpec::ValidateRun { plan_id: id.clone(), kind: "rowcount".into() },
    )
    .await
}

/// `POST /databridge/plans/{id}/cutover` — guarded (admin): validated + last validation passed +
/// CDC lag under threshold.
async fn db_cutover(
    State(s): State<AppState>,
    Extension(actor): Extension<Actor>,
    Path(id): Path<String>,
) -> AppResult<(StatusCode, Json<Value>)> {
    crate::auth::require_role(s.config.auth_required, &actor, crate::auth::ROLE_ADMIN)?;
    let plan = atlas_inventory::databridge::plans::get_plan(&s.pool, &id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("plan {id}")))?;
    if plan.state != "validated" {
        return Err(AppError::Conflict(format!(
            "plan must be validated before cutover (state: {})",
            plan.state
        )));
    }
    let last = atlas_inventory::databridge::validations::latest_for_plan(&s.pool, &id).await?;
    if last.as_ref().map(|v| v.state.as_str()) != Some("passed") {
        return Err(AppError::Conflict(
            "latest validation must have passed before cutover".into(),
        ));
    }
    if let Some(cdc_id) = plan.cdc_stream_id.as_deref() {
        if let Some(stream) = atlas_inventory::databridge::cdc::get_stream(&s.pool, cdc_id).await? {
            if stream.lag_seconds > CUTOVER_MAX_LAG_SECS {
                return Err(AppError::Conflict(format!(
                    "CDC lag ({}s) exceeds the cutover threshold ({CUTOVER_MAX_LAG_SECS}s); wait for it to drain",
                    stream.lag_seconds
                )));
            }
        }
    }
    let job_id = ids::job_id();
    let job = s
        .jobs
        .enqueue(&job_id, "global", &actor.id, JobSpec::Cutover { plan_id: id.clone() }, None)
        .await
        .map_err(AppError::from)?;
    let _ = atlas_inventory::audit::record(
        &s.pool, None, &actor.id, "databridge.cutover", "migration.plan", &id, "accepted", None, None,
    )
    .await;
    Ok(accepted(&job, json!({ "plan_id": id })))
}

/// `POST /databridge/plans/{id}/rollback` — guarded (admin): only within the rollback window.
async fn db_rollback(
    State(s): State<AppState>,
    Extension(actor): Extension<Actor>,
    Path(id): Path<String>,
) -> AppResult<(StatusCode, Json<Value>)> {
    crate::auth::require_role(s.config.auth_required, &actor, crate::auth::ROLE_ADMIN)?;
    atlas_inventory::databridge::plans::get_plan(&s.pool, &id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("plan {id}")))?;
    let cut = atlas_inventory::databridge::cutovers::latest_for_plan(&s.pool, &id)
        .await?
        .ok_or_else(|| AppError::Conflict("no cutover to roll back".into()))?;
    if let Some(deadline) = cut.rollback_deadline.as_deref() {
        if let Ok(dl) = chrono::DateTime::parse_from_rfc3339(deadline) {
            if chrono::Utc::now() > dl {
                return Err(AppError::Conflict(
                    "rollback window has closed for this cutover".into(),
                ));
            }
        }
    }
    let job_id = ids::job_id();
    let job = s
        .jobs
        .enqueue(&job_id, "global", &actor.id, JobSpec::Rollback { plan_id: id.clone() }, None)
        .await
        .map_err(AppError::from)?;
    Ok(accepted(&job, json!({ "plan_id": id })))
}

async fn db_list_cdc_streams(State(s): State<AppState>) -> AppResult<Json<Value>> {
    Ok(Json(json!(
        atlas_inventory::databridge::cdc::list_streams(&s.pool).await?
    )))
}

async fn db_get_cdc_stream(
    State(s): State<AppState>,
    Path(id): Path<String>,
) -> AppResult<Json<Value>> {
    let c = atlas_inventory::databridge::cdc::get_stream(&s.pool, &id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("cdc stream {id}")))?;
    Ok(Json(json!(c)))
}

#[derive(Debug, Deserialize)]
struct PlanIdQuery {
    plan_id: Option<String>,
}

async fn db_list_validations(
    State(s): State<AppState>,
    Query(q): Query<PlanIdQuery>,
) -> AppResult<Json<Value>> {
    Ok(Json(json!(
        atlas_inventory::databridge::validations::list_validations(&s.pool, q.plan_id.as_deref())
            .await?
    )))
}

async fn db_list_cutovers(State(s): State<AppState>) -> AppResult<Json<Value>> {
    Ok(Json(json!(
        atlas_inventory::databridge::cutovers::list_cutovers(&s.pool).await?
    )))
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

/// `GET /buckets/{id}/stats` — RGW usage + quota for the bucket (via `radosgw-admin bucket stats`).
async fn bucket_stats(State(s): State<AppState>, Path(id): Path<String>) -> AppResult<Json<Value>> {
    let b = atlas_inventory::buckets::get_bucket(&s.pool, &id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("bucket {id}")))?;
    let name = b
        .bucket_name
        .ok_or_else(|| AppError::Validation("bucket has no bucket_name".into()))?;
    let stats = atlas_driver_ceph::radosgw_admin_json(&["bucket", "stats", "--bucket", &name])
        .await
        .map_err(|e| AppError::Driver(e.to_string()))?;
    let main = stats
        .get("usage")
        .and_then(|u| u.get("rgw.main"))
        .cloned()
        .unwrap_or(Value::Null);
    Ok(Json(json!({
        "bucket_id": id, "bucket": name,
        "num_objects": main.get("num_objects").cloned().unwrap_or(json!(0)),
        "size_bytes": main.get("size_actual").cloned().unwrap_or(json!(0)),
        "quota": stats.get("bucket_quota").cloned().unwrap_or(Value::Null)
    })))
}

#[derive(Debug, Deserialize)]
struct PrefixQuery {
    prefix: Option<String>,
}

#[derive(serde::Deserialize)]
struct ObjectKeyQuery {
    key: Option<String>,
    ttl_secs: Option<u64>,
}

/// Build an `S3Target` for a bound bucket, reading the OBC credentials from its in-cluster
/// Secret. The endpoint prefers `rgw_public_endpoint` (the browser-reachable URL) so presigned
/// upload/download URLs it mints are usable from outside the cluster. Shared by every object op.
async fn bucket_s3_target(
    s: &AppState,
    id: &str,
) -> AppResult<atlas_driver_rgw::S3Target> {
    let b = atlas_inventory::buckets::get_bucket(&s.pool, id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("bucket {id}")))?;
    if b.state != "bound" {
        return Err(AppError::Validation("bucket is not bound".into()));
    }
    let k8s = s
        .k8s
        .as_ref()
        .ok_or_else(|| AppError::Driver("no reachable Kubernetes cluster".into()))?;
    let ns = b.namespace.clone().unwrap_or_else(|| "rook-ceph".into());
    let secret_ref = b
        .secret_ref
        .clone()
        .ok_or_else(|| AppError::Validation("bucket has no secret".into()))?;
    let secret = k8s
        .get_secret(&ns, &secret_ref)
        .await
        .map_err(|e| AppError::Driver(e.to_string()))?
        .ok_or_else(|| AppError::NotFound(format!("bucket secret {secret_ref}")))?;
    let access = secret
        .get("AWS_ACCESS_KEY_ID")
        .ok_or_else(|| AppError::Internal("bucket secret missing AWS_ACCESS_KEY_ID".into()))?;
    let secret_key = secret
        .get("AWS_SECRET_ACCESS_KEY")
        .ok_or_else(|| AppError::Internal("bucket secret missing AWS_SECRET_ACCESS_KEY".into()))?;
    let endpoint = s
        .config
        .rgw_public_endpoint
        .clone()
        .or(b.endpoint)
        .unwrap_or_default();
    atlas_driver_rgw::S3Target::new(
        &endpoint,
        &b.region.unwrap_or_else(|| "us-east-1".into()),
        &b.bucket_name.unwrap_or_default(),
        access,
        secret_key,
    )
    .map_err(|e| AppError::Driver(e.to_string()))
}

/// `GET /buckets/{id}/objects[?prefix=]` — list objects in the bucket over S3 (creds in-cluster).
async fn bucket_objects(
    State(s): State<AppState>,
    Path(id): Path<String>,
    Query(q): Query<PrefixQuery>,
) -> AppResult<Json<Value>> {
    let s3 = bucket_s3_target(&s, &id).await?;
    let objects = s3
        .list_objects(q.prefix.as_deref())
        .await
        .map_err(|e| AppError::Driver(e.to_string()))?;
    let items: Vec<Value> = objects
        .into_iter()
        .map(|(key, size)| json!({ "key": key, "size_bytes": size }))
        .collect();
    Ok(Json(
        json!({ "bucket_id": id, "count": items.len(), "objects": items }),
    ))
}

/// TTL for minted object upload/download URLs; long enough for a large db file over a slow link.
const OBJECT_URL_TTL_SECS: u64 = 3600;

/// `POST /buckets/{id}/objects/upload-url` — mint a presigned PUT URL so the browser uploads a
/// file straight to RGW (the gateway never touches the bytes). Body: `{ "key": "...", "ttl_secs"? }`.
async fn bucket_object_upload_url(
    State(s): State<AppState>,
    Extension(actor): Extension<Actor>,
    Path(id): Path<String>,
    Json(body): Json<Value>,
) -> AppResult<Json<Value>> {
    crate::auth::require_role(s.config.auth_required, &actor, crate::auth::ROLE_OPERATOR)?;
    let key = body
        .get("key")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|k| !k.is_empty())
        .ok_or_else(|| AppError::Validation("object key is required".into()))?
        .to_string();
    let ttl = body
        .get("ttl_secs")
        .and_then(|v| v.as_u64())
        .unwrap_or(OBJECT_URL_TTL_SECS);
    // Versioned uploads (for db-file backups): store each upload at `<key>.<UTC-timestamp>` so old
    // copies are retained instead of overwritten. The timestamp is lexicographically sortable, so
    // the prune endpoint can keep the newest N by a plain string sort. base_key + "." is the prefix
    // that lists all versions of this key.
    let versioned = body
        .get("versioned")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let stored_key = if versioned {
        format!("{key}.{}", chrono::Utc::now().format("%Y%m%dT%H%M%SZ"))
    } else {
        key.clone()
    };
    let s3 = bucket_s3_target(&s, &id).await?;
    let url = s3.presigned_put(&stored_key, ttl);
    let _ = atlas_inventory::audit::record(
        &s.pool,
        None,
        &actor.id,
        "bucket.object.upload-url",
        "bucket.object",
        &format!("{id}/{stored_key}"),
        "success",
        None,
        None,
    )
    .await;
    Ok(Json(json!({
        "bucket_id": id,
        "key": stored_key,
        "base_key": key,
        "versioned": versioned,
        "method": "PUT",
        "url": url,
        "expires_in": ttl,
    })))
}

/// `GET /buckets/{id}/objects/download-url?key=...[&ttl_secs=]` — mint a presigned GET URL so the
/// browser downloads an object straight from RGW.
async fn bucket_object_download_url(
    State(s): State<AppState>,
    Path(id): Path<String>,
    Query(q): Query<ObjectKeyQuery>,
) -> AppResult<Json<Value>> {
    let key = q
        .key
        .as_deref()
        .map(str::trim)
        .filter(|k| !k.is_empty())
        .ok_or_else(|| AppError::Validation("object key is required".into()))?;
    let ttl = q.ttl_secs.unwrap_or(OBJECT_URL_TTL_SECS);
    let s3 = bucket_s3_target(&s, &id).await?;
    let url = s3.presigned_get(key, ttl);
    Ok(Json(
        json!({ "bucket_id": id, "key": key, "method": "GET", "url": url, "expires_in": ttl }),
    ))
}

/// `DELETE /buckets/{id}/objects?key=...` — delete a single object (proxied through the gateway so
/// it stays authenticated + audited; the payload is tiny so there's no streaming concern).
async fn bucket_object_delete(
    State(s): State<AppState>,
    Extension(actor): Extension<Actor>,
    Path(id): Path<String>,
    Query(q): Query<ObjectKeyQuery>,
) -> AppResult<Json<Value>> {
    crate::auth::require_role(s.config.auth_required, &actor, crate::auth::ROLE_OPERATOR)?;
    let key = q
        .key
        .as_deref()
        .map(str::trim)
        .filter(|k| !k.is_empty())
        .ok_or_else(|| AppError::Validation("object key is required".into()))?;
    let s3 = bucket_s3_target(&s, &id).await?;
    s3.delete_object(key)
        .await
        .map_err(|e| AppError::Driver(e.to_string()))?;
    let _ = atlas_inventory::audit::record(
        &s.pool,
        None,
        &actor.id,
        "bucket.object.delete",
        "bucket.object",
        &format!("{id}/{key}"),
        "success",
        None,
        None,
    )
    .await;
    Ok(Json(json!({ "bucket_id": id, "key": key, "deleted": true })))
}

/// `POST /buckets/{id}/objects/prune` — retention for versioned db-file backups. Body:
/// `{ "prefix": "<base_key>.", "keep": N }`. Lists objects under `prefix`, keeps the newest N
/// (version suffixes are sortable UTC timestamps → lexicographic desc = newest first) and deletes
/// the rest. Returns the deleted keys. Call it after a versioned upload to enforce keep-N.
async fn bucket_objects_prune(
    State(s): State<AppState>,
    Extension(actor): Extension<Actor>,
    Path(id): Path<String>,
    Json(body): Json<Value>,
) -> AppResult<Json<Value>> {
    crate::auth::require_role(s.config.auth_required, &actor, crate::auth::ROLE_OPERATOR)?;
    let prefix = body
        .get("prefix")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|p| !p.is_empty())
        .ok_or_else(|| AppError::Validation("prefix is required".into()))?
        .to_string();
    let keep = body.get("keep").and_then(|v| v.as_u64()).unwrap_or(5) as usize;
    let s3 = bucket_s3_target(&s, &id).await?;
    let mut objs = s3
        .list_objects(Some(&prefix))
        .await
        .map_err(|e| AppError::Driver(e.to_string()))?;
    objs.sort_by(|a, b| b.0.cmp(&a.0)); // newest (highest timestamp suffix) first
    let mut pruned = Vec::new();
    for (key, _size) in objs.into_iter().skip(keep) {
        s3.delete_object(&key)
            .await
            .map_err(|e| AppError::Driver(e.to_string()))?;
        pruned.push(key);
    }
    let _ = atlas_inventory::audit::record(
        &s.pool,
        None,
        &actor.id,
        "bucket.object.prune",
        "bucket.object",
        &format!("{id}/{prefix} keep={keep} pruned={}", pruned.len()),
        "success",
        None,
        None,
    )
    .await;
    Ok(Json(json!({
        "bucket_id": id, "prefix": prefix, "keep": keep,
        "pruned_count": pruned.len(), "pruned": pruned,
    })))
}

/// `DELETE /buckets/{id}[?force=true]` — delete the OBC + row; blocked if backups reference it.
async fn delete_bucket(
    State(s): State<AppState>,
    Extension(actor): Extension<Actor>,
    Path(id): Path<String>,
    Query(q): Query<ForceParams>,
) -> AppResult<(StatusCode, Json<Value>)> {
    crate::auth::require_role(s.config.auth_required, &actor, crate::auth::ROLE_OPERATOR)?;
    let bucket = atlas_inventory::buckets::get_bucket(&s.pool, &id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("bucket {id}")))?;

    let deps = atlas_inventory::backups::count_for_bucket(&s.pool, &id).await?;
    if deps > 0 && !q.force {
        return Err(AppError::Conflict(format!(
            "bucket {id} still holds {deps} backup(s); delete them first or pass ?force=true"
        )));
    }

    let spec = JobSpec::BucketDelete {
        bucket_id: id.clone(),
        namespace: bucket.namespace.unwrap_or_else(|| "rook-ceph".into()),
        // The OBC name equals the bucket's registered name (set at creation).
        obc_name: bucket.name,
    };
    let job_id = ids::job_id();
    let job = s
        .jobs
        .enqueue(&job_id, &bucket.tenant_id, &actor.id, spec, None)
        .await
        .map_err(AppError::from)?;
    let _ = atlas_inventory::audit::record(
        &s.pool,
        None,
        &actor.id,
        "bucket.delete.requested",
        "bucket",
        &id,
        "accepted",
        None,
        None,
    )
    .await;
    Ok(accepted(&job, json!({ "bucket_id": id })))
}

#[derive(Debug, Deserialize)]
struct CreateBucketBody {
    name: String,
    namespace: Option<String>,
    storage_class: Option<String>,
    /// Optional RGW quota: max object count.
    max_objects: Option<i64>,
    /// Optional RGW quota: max size (e.g. "2G").
    max_size: Option<String>,
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
        max_objects: body.max_objects,
        max_size: body.max_size.clone(),
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

#[derive(Debug, Deserialize)]
struct ListBackupsQuery {
    volume_id: Option<String>,
}

async fn list_backups(
    State(s): State<AppState>,
    Query(q): Query<ListBackupsQuery>,
) -> AppResult<Json<Value>> {
    Ok(Json(json!(
        atlas_inventory::backups::list_backups(&s.pool, q.volume_id.as_deref()).await?
    )))
}

async fn get_backup(State(s): State<AppState>, Path(id): Path<String>) -> AppResult<Json<Value>> {
    let b = atlas_inventory::backups::get_backup(&s.pool, &id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("backup {id}")))?;
    Ok(Json(json!(b)))
}

/// `DELETE /backups/{id}` — remove the backup's S3 objects + RBD snapshot + row (async job).
async fn delete_backup(
    State(s): State<AppState>,
    Extension(actor): Extension<Actor>,
    Path(id): Path<String>,
) -> AppResult<(StatusCode, Json<Value>)> {
    crate::auth::require_role(s.config.auth_required, &actor, crate::auth::ROLE_OPERATOR)?;
    let backup = atlas_inventory::backups::get_backup(&s.pool, &id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("backup {id}")))?;
    let bucket = atlas_inventory::buckets::get_bucket(&s.pool, &backup.bucket_id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("bucket {}", backup.bucket_id)))?;

    // The source volume gives the namespace/pvc for best-effort RBD snapshot cleanup.
    let vol = atlas_inventory::get_volume(&s.pool, &backup.volume_id).await?;
    let volume_namespace = vol
        .as_ref()
        .and_then(|v| v.kubernetes_namespace.clone())
        .unwrap_or_default();
    let pvc_name = vol.and_then(|v| v.pvc_name).unwrap_or_default();

    let spec = make_backup_delete_spec(&backup, bucket, volume_namespace, pvc_name);
    let job_id = ids::job_id();
    let job = s
        .jobs
        .enqueue(&job_id, &backup.tenant_id, &actor.id, spec, None)
        .await
        .map_err(AppError::from)?;
    let _ = atlas_inventory::audit::record(
        &s.pool,
        None,
        &actor.id,
        "backup.delete.requested",
        "backup",
        &id,
        "accepted",
        None,
        None,
    )
    .await;
    Ok(accepted(&job, json!({ "backup_id": id })))
}

#[derive(Debug, Deserialize)]
struct DownloadQuery {
    /// "manifest" (default) or "data" (the `.rbd-diff` object).
    what: Option<String>,
}

/// `GET /backups/{id}/download?what=data|manifest` — a time-limited presigned S3 URL for the
/// backup object, so a client downloads it straight from RGW (no proxy, no credentials).
async fn download_backup(
    State(s): State<AppState>,
    Path(id): Path<String>,
    Query(q): Query<DownloadQuery>,
) -> AppResult<Json<Value>> {
    let backup = atlas_inventory::backups::get_backup(&s.pool, &id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("backup {id}")))?;
    let bucket = atlas_inventory::buckets::get_bucket(&s.pool, &backup.bucket_id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("bucket {}", backup.bucket_id)))?;
    if bucket.state != "bound" {
        return Err(AppError::Validation("bucket is not bound".into()));
    }
    let key = match q.what.as_deref() {
        Some("data") => format!("{}.rbd-diff", backup.object_key),
        _ => backup.object_key.clone(),
    };

    // Sign with the bucket's credentials, read in-cluster (never returned to the caller).
    let k8s = s
        .k8s
        .as_ref()
        .ok_or_else(|| AppError::Driver("no reachable Kubernetes cluster".into()))?;
    let ns = bucket
        .namespace
        .clone()
        .unwrap_or_else(|| "rook-ceph".into());
    let secret_ref = bucket
        .secret_ref
        .clone()
        .ok_or_else(|| AppError::Validation("bucket has no secret".into()))?;
    let secret = k8s
        .get_secret(&ns, &secret_ref)
        .await
        .map_err(|e| AppError::Driver(e.to_string()))?
        .ok_or_else(|| AppError::NotFound(format!("bucket secret {secret_ref}")))?;
    let access = secret
        .get("AWS_ACCESS_KEY_ID")
        .ok_or_else(|| AppError::Internal("bucket secret missing AWS_ACCESS_KEY_ID".into()))?;
    let secret_key = secret
        .get("AWS_SECRET_ACCESS_KEY")
        .ok_or_else(|| AppError::Internal("bucket secret missing AWS_SECRET_ACCESS_KEY".into()))?;

    // Prefer the configured public RGW endpoint so the presigned URL is reachable off-cluster;
    // the signature binds to this host, so the client must connect to the same endpoint.
    let endpoint = s
        .config
        .rgw_public_endpoint
        .clone()
        .or(bucket.endpoint)
        .unwrap_or_default();
    let s3 = atlas_driver_rgw::S3Target::new(
        &endpoint,
        &bucket.region.unwrap_or_else(|| "us-east-1".into()),
        &bucket.bucket_name.unwrap_or_default(),
        access,
        secret_key,
    )
    .map_err(|e| AppError::Driver(e.to_string()))?;
    let ttl_secs = 900;
    let url = s3.presigned_get(&key, ttl_secs);
    Ok(Json(json!({
        "url": url, "object_key": key, "expires_in_secs": ttl_secs
    })))
}

/// Build a `BackupDelete` job spec for a backup + its bucket.
fn make_backup_delete_spec(
    backup: &atlas_api_types::BackupRecord,
    bucket: atlas_api_types::StorageBucket,
    volume_namespace: String,
    pvc_name: String,
) -> JobSpec {
    JobSpec::BackupDelete {
        backup_id: backup.id.clone(),
        manifest_key: backup.object_key.clone(),
        data_key: format!("{}.rbd-diff", backup.object_key),
        volume_namespace,
        pvc_name,
        rbd_snap: format!("atlasbkp-{}", &backup.id[4..]),
        bucket_namespace: bucket.namespace.unwrap_or_else(|| "rook-ceph".into()),
        bucket_secret_ref: bucket.secret_ref.unwrap_or_default(),
        bucket_endpoint: bucket.endpoint.unwrap_or_default(),
        bucket_name: bucket.bucket_name.unwrap_or_default(),
        bucket_region: bucket.region.unwrap_or_else(|| "us-east-1".into()),
    }
}

/// Enqueue a backup-delete job for one backup (resolves its bucket + volume placement first).
/// No-op if the bucket is gone/unbound.
async fn enqueue_backup_delete(s: &AppState, actor_id: &str, old: &atlas_api_types::BackupRecord) {
    let bucket = match atlas_inventory::buckets::get_bucket(&s.pool, &old.bucket_id).await {
        Ok(Some(b)) if b.state == "bound" => b,
        _ => return,
    };
    let vol = atlas_inventory::get_volume(&s.pool, &old.volume_id)
        .await
        .ok()
        .flatten();
    let ns = vol
        .as_ref()
        .and_then(|v| v.kubernetes_namespace.clone())
        .unwrap_or_default();
    let pvc = vol.and_then(|v| v.pvc_name).unwrap_or_default();
    let spec = make_backup_delete_spec(old, bucket, ns, pvc);
    let _ = s
        .jobs
        .enqueue(&ids::job_id(), &old.tenant_id, actor_id, spec, None)
        .await;
}

/// Retention: prune backups for a volume beyond the `keep` most recent (enqueues delete jobs).
async fn prune_backups(s: &AppState, actor_id: &str, volume_id: &str, keep: i64) {
    if keep <= 0 {
        return;
    }
    let all = match atlas_inventory::backups::list_backups(&s.pool, Some(volume_id)).await {
        Ok(a) => a,
        Err(_) => return,
    };
    for old in all.into_iter().skip(keep as usize) {
        enqueue_backup_delete(s, actor_id, &old).await;
    }
}

/// Retention: prune backups for a volume older than `max_age_secs` (enqueues delete jobs).
async fn prune_backups_by_age(s: &AppState, actor_id: &str, volume_id: &str, max_age_secs: i64) {
    if max_age_secs <= 0 {
        return;
    }
    let cutoff = (chrono::Utc::now() - chrono::Duration::seconds(max_age_secs))
        .format("%Y-%m-%dT%H:%M:%S%.3fZ")
        .to_string();
    let old = match atlas_inventory::backups::list_older_than(&s.pool, volume_id, &cutoff).await {
        Ok(o) => o,
        Err(_) => return,
    };
    for b in &old {
        enqueue_backup_delete(s, actor_id, b).await;
    }
}

#[derive(Debug, Deserialize)]
struct CreateBackupBody {
    volume_id: String,
    bucket_id: String,
    /// "manifest" (default) or "data" (also exports the RBD image data to S3).
    #[serde(default)]
    mode: Option<String>,
    /// Retain only the most recent `keep` backups for this volume (0/absent = config default).
    #[serde(default)]
    keep: Option<i64>,
    /// Prune backups older than this many seconds (0/absent = config default).
    #[serde(default)]
    max_age_secs: Option<i64>,
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

    // Retention: prune older backups for this volume beyond the keep count and/or past max age.
    let keep = body.keep.unwrap_or(s.config.backup_keep);
    prune_backups(&s, &actor.id, &body.volume_id, keep).await;
    let max_age = body.max_age_secs.unwrap_or(s.config.backup_max_age_secs);
    prune_backups_by_age(&s, &actor.id, &body.volume_id, max_age).await;

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
    /// "snapshot" (default) or "data" (reconstruct from the RBD diff in S3).
    #[serde(default)]
    mode: Option<String>,
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
        mode: body.mode.clone().unwrap_or_else(|| "snapshot".into()),
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
