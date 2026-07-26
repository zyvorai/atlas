// Copyright (c) 2026 ZyvorAI Labs Private Limited. All rights reserved.
use axum::{
    extract::{Path, Query, State},
    Json,
};
use serde::Deserialize;
use serde_json::{json, Value};

use atlas_api_types::{Capabilities, StorageClassInfo};
use atlas_common::{AppError, AppResult};

use crate::state::AppState;
use super::util::CEPH_BACKEND_ID;

// ---- clusters / inventory ----

pub(crate) async fn list_clusters(State(s): State<AppState>) -> AppResult<Json<Value>> {
    Ok(Json(json!(atlas_inventory::list_clusters(&s.pool).await?)))
}

pub(crate) async fn cluster_health(
    State(s): State<AppState>,
    Path(id): Path<String>,
) -> AppResult<Json<Value>> {
    let h = atlas_inventory::cluster_health(&s.pool, &id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("cluster {id}")))?;
    Ok(Json(json!(h)))
}

pub(crate) async fn cluster_capabilities(
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
pub(crate) async fn list_nodes(State(s): State<AppState>) -> AppResult<Json<Value>> {
    let osds = atlas_inventory::list_osds(&s.pool).await?;
    let mut hosts: Vec<String> = osds.into_iter().filter_map(|o| o.host).collect();
    hosts.sort();
    hosts.dedup();
    let nodes: Vec<Value> = hosts.into_iter().map(|h| json!({ "host": h })).collect();
    Ok(Json(json!(nodes)))
}

pub(crate) async fn list_osds(State(s): State<AppState>) -> AppResult<Json<Value>> {
    Ok(Json(json!(atlas_inventory::list_osds(&s.pool).await?)))
}

/// `GET /ceph/status` — live `ceph status` from the cluster (health, quorum, osdmap, pgmap, I/O).
pub(crate) async fn get_ceph_status(State(s): State<AppState>) -> AppResult<Json<Value>> {
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
pub(crate) async fn get_ceph_osd_tree(State(s): State<AppState>) -> AppResult<Json<Value>> {
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
pub(crate) async fn get_ceph_df(State(s): State<AppState>) -> AppResult<Json<Value>> {
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
pub(crate) async fn get_ceph_osd_df(State(s): State<AppState>) -> AppResult<Json<Value>> {
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
pub(crate) struct PoolQuery {
    backend: Option<String>,
    kind: Option<String>,
}

pub(crate) async fn list_pools(
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
pub(crate) struct VolumeQuery {
    state: Option<String>,
    tenant: Option<String>,
    backend: Option<String>,
    kind: Option<String>,
}

pub(crate) async fn list_volumes(
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
pub(crate) async fn volumes_csv(
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
        // Neutralize CSV formula injection: a cell starting with =/+/-/@ can be interpreted as a
        // formula by spreadsheet apps (Excel/Sheets) when this export is opened.
        let guarded = if matches!(v.as_bytes().first(), Some(b'=' | b'+' | b'-' | b'@')) {
            format!("'{v}")
        } else {
            v.to_string()
        };
        if guarded.contains([',', '"', '\n']) {
            let _ = write!(o, "\"{}\"", guarded.replace('"', "\"\""));
        } else {
            let _ = write!(o, "{guarded}");
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

pub(crate) async fn get_volume(State(s): State<AppState>, Path(id): Path<String>) -> AppResult<Json<Value>> {
    let v = atlas_inventory::get_volume(&s.pool, &id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("volume {id}")))?;
    Ok(Json(json!(v)))
}
// ---- live Kubernetes ----

pub(crate) async fn list_storage_classes(State(s): State<AppState>) -> AppResult<Json<Vec<StorageClassInfo>>> {
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

pub(crate) async fn list_pvcs(State(s): State<AppState>) -> AppResult<Json<Value>> {
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

pub(crate) async fn list_pvs(State(s): State<AppState>) -> AppResult<Json<Value>> {
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
