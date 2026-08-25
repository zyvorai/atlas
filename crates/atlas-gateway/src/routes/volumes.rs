// Copyright (c) 2026 ZyvorAI Labs Private Limited. All rights reserved.
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Extension, Json,
};
use serde::Deserialize;
use serde_json::{json, Value};

use atlas_api_types::{CreateVolumeRequest, Owner, Placement};
use atlas_common::{ids, AppError, AppResult};
use atlas_jobs::{JobSpec, OwnerRef};

use crate::auth::Actor;
use crate::state::AppState;
use super::util::{accepted, validate_k8s_name, CEPH_BACKEND_ID};

// ---- snapshots (read) ----

pub(crate) async fn list_snapshots(State(s): State<AppState>) -> AppResult<Json<Value>> {
    Ok(Json(json!(
        atlas_inventory::snapshots::list_snapshots(&s.pool, None).await?
    )))
}

/// `POST /volumes` — create a Ceph-backed PVC as an async job (PDF §8.2).
pub(crate) async fn create_volume(
    State(s): State<AppState>,
    Extension(actor): Extension<Actor>,
    Json(body): Json<CreateVolumeRequest>,
) -> AppResult<(StatusCode, Json<Value>)> {
    crate::auth::require_role(s.config.auth_required, &actor, crate::auth::ROLE_OPERATOR)?;
    // A tenant-scoped operator must not create a volume attributed to (and billed against the
    // quota of) a DIFFERENT tenant by simply naming it in the request body — `tenant_id` here is
    // caller-supplied, unlike the read/single-resource paths where it's looked up from inventory.
    if let Some(scope) = crate::auth::tenant_scope(s.config.auth_required, &actor) {
        if body.tenant_id != scope {
            return Err(AppError::Forbidden(format!(
                "actor '{}' may only create volumes for tenant '{scope}'",
                actor.id
            )));
        }
    }
    if body.name.trim().is_empty() {
        return Err(AppError::Validation("name is required".into()));
    }
    validate_k8s_name(&body.name)?;
    if body.size_bytes <= 0 {
        return Err(AppError::Validation("size_bytes must be > 0".into()));
    }

    // Maintenance: a cordoned backend rejects new provisioning (existing volumes are untouched).
    if atlas_inventory::is_backend_cordoned(&s.pool, CEPH_BACKEND_ID).await? {
        return Err(AppError::Unavailable(format!(
            "backend {CEPH_BACKEND_ID} is cordoned for maintenance"
        )));
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
    // Per-tenant policy override (PDF §14): unless the request pins an explicit StorageClass, a
    // tenant's override for this intent wins over the built-in catalog — and, since
    // `PUT /tenants/{id}/policies/{intent}` isn't restricted to the built-in catalog, this can
    // resolve an `intent` that `atlas_policy::resolve` doesn't recognize at all.
    let tenant_override = if sc_override.is_none() {
        match body.policy.as_deref() {
            Some(intent) => {
                atlas_inventory::tenants::get_policy(&s.pool, &body.tenant_id, intent).await?
            }
            None => None,
        }
    } else {
        None
    };
    let mut placement = match atlas_policy::resolve(
        body.policy.as_deref(),
        body.kind,
        sc_override.as_deref(),
    ) {
        Ok(p) => p,
        // Unrecognized by the built-in catalog, but the tenant has its own override for this
        // intent — the fields below get overwritten from `tenant_override` immediately after.
        Err(_) if tenant_override.is_some() => Placement {
            intent: body.policy.clone().unwrap_or_default(),
            storage_class: String::new(),
            access_mode: String::new(),
            volume_mode: String::new(),
            kind: body.kind,
        },
        Err(e) => return Err(AppError::Validation(e)),
    };
    if let Some(tp) = tenant_override {
        placement.storage_class = tp.storage_class;
        placement.access_mode = tp.access_mode;
        placement.volume_mode = tp.volume_mode;
        // `tenant_policies` has no `kind` column, so `placement.kind` (already set from the
        // built-in policy's kind, or the request's kind if the intent isn't a known built-in) is
        // the best available signal here — still correct for the common case of a tenant
        // overriding just the StorageClass for an existing named intent.
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
        // Use the *resolved* kind, not the raw request's — a named policy (e.g. `shared`) always
        // implies a specific kind (CephFS/`Filesystem`) regardless of what the client passed or
        // defaulted to; trusting `body.kind` here mis-tags the volume and it gets silently pruned
        // by discovery on drivers that only enumerate one storage kind.
        kind: format!("{:?}", placement.kind).to_lowercase(),
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

/// `DELETE /volumes/{id}[?confirm=true]` — delete the PVC + inventory row as a job.
/// Safe-by-default (PDF §14 Rule 2): production-class volumes require confirm=true
/// (legacy force=true also accepted — matches the console delete path).
pub(crate) async fn delete_volume(
    State(s): State<AppState>,
    Extension(actor): Extension<Actor>,
    Path(id): Path<String>,
    Query(q): Query<ConfirmParams>,
) -> AppResult<(StatusCode, Json<Value>)> {
    crate::auth::require_role(s.config.auth_required, &actor, crate::auth::ROLE_ADMIN)?;
    let vol = atlas_inventory::get_volume(&s.pool, &id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("volume {id}")))?;
    if volume_requires_delete_confirm(&vol) && !q.confirmed() {
        return Err(AppError::Validation(
            "confirm=true is required to delete this volume (production / protected class)".into(),
        ));
    }
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
pub(crate) struct ExpandBody {
    new_size_bytes: i64,
}

/// `POST /volumes/{id}/expand`
pub(crate) async fn expand_volume(
    State(s): State<AppState>,
    Extension(actor): Extension<Actor>,
    Path(id): Path<String>,
    Json(body): Json<ExpandBody>,
) -> AppResult<(StatusCode, Json<Value>)> {
    crate::auth::require_role(s.config.auth_required, &actor, crate::auth::ROLE_OPERATOR)?;
    let vol = atlas_inventory::get_volume(&s.pool, &id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("volume {id}")))?;
    let resource_tenant = atlas_inventory::volume_tenant(&s.pool, &id).await?;
    crate::auth::require_tenant(s.config.auth_required, &actor, &resource_tenant, format!("volume {id}"))?;
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
pub(crate) struct SnapshotBody {
    name: Option<String>,
    snapshot_class: Option<String>,
}

/// `POST /volumes/{id}/snapshots`
pub(crate) async fn create_snapshot(
    State(s): State<AppState>,
    Extension(actor): Extension<Actor>,
    Path(id): Path<String>,
    Json(body): Json<SnapshotBody>,
) -> AppResult<(StatusCode, Json<Value>)> {
    crate::auth::require_role(s.config.auth_required, &actor, crate::auth::ROLE_OPERATOR)?;
    let vol = atlas_inventory::get_volume(&s.pool, &id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("volume {id}")))?;
    let resource_tenant = atlas_inventory::volume_tenant(&s.pool, &id).await?;
    crate::auth::require_tenant(s.config.auth_required, &actor, &resource_tenant, format!("volume {id}"))?;
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
pub(crate) struct ForceParams {
    #[serde(default)]
    pub(crate) force: bool,
}

/// Explicit destructive confirm for volume delete (PDF §14 Rule 2).
#[derive(Debug, Deserialize, Default)]
pub(crate) struct ConfirmParams {
    #[serde(default)]
    pub(crate) confirm: bool,
    /// Legacy alias used by the console (`?force=true`).
    #[serde(default)]
    pub(crate) force: bool,
}

impl ConfirmParams {
    pub(crate) fn confirmed(&self) -> bool {
        self.confirm || self.force
    }
}

fn volume_requires_delete_confirm(vol: &atlas_api_types::StorageVolume) -> bool {
    let sc = vol.storage_class_name.as_deref().unwrap_or("").to_ascii_lowercase();
    // Production / protected classes (zyvor-rbd-prod, *-production*, etc.).
    sc.contains("prod") || sc.contains("production") || sc.contains("database")
}

/// `DELETE /snapshots/{id}[?force=true]` — blocked if the snapshot has dependent clones (PDF §8.3).
pub(crate) async fn delete_snapshot(
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
    crate::auth::require_tenant(s.config.auth_required, &actor, &snap.tenant_id, format!("snapshot {id}"))?;

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
pub(crate) struct CloneBody {
    /// New volume/PVC name (required for clone; optional for restore).
    name: Option<String>,
    namespace: Option<String>,
    storage_class: Option<String>,
    size_bytes: Option<i64>,
    #[serde(default)]
    owner: Option<Owner>,
}

/// `POST /snapshots/{id}/clone` — provision a new independent volume from a snapshot (PDF §8.3).
pub(crate) async fn clone_snapshot(
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
pub(crate) async fn restore_snapshot(
    State(s): State<AppState>,
    Extension(actor): Extension<Actor>,
    Path(id): Path<String>,
    Json(body): Json<CloneBody>,
) -> AppResult<(StatusCode, Json<Value>)> {
    // Default restore name derives from the snapshot id when not supplied. `id` is client-supplied
    // (URL path), so strip the "snap_" prefix defensively instead of slicing — a short/malformed id
    // would otherwise panic before the not-found check below runs.
    let name = body
        .name
        .clone()
        .filter(|n| !n.trim().is_empty())
        .unwrap_or_else(|| format!("restore-{}", id.strip_prefix("snap_").unwrap_or(&id)));
    enqueue_clone(&s, &actor, &id, "restore", name, body).await
}

pub(crate) async fn enqueue_clone(
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
    crate::auth::require_tenant(s.config.auth_required, actor, &snap.tenant_id, format!("snapshot {snapshot_id}"))?;
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
    if size_bytes <= 0 {
        return Err(AppError::Validation("size_bytes must be > 0".into()));
    }

    // Tenant quota admission (PDF §14): a clone/restore provisions a new volume just like
    // `POST /volumes` does, so it must be admission-checked the same way.
    match atlas_inventory::tenants::check_admission(&s.pool, &snap.tenant_id, size_bytes).await? {
        atlas_inventory::tenants::QuotaCheck::Ok => {}
        atlas_inventory::tenants::QuotaCheck::Bytes { limit, would_be } => {
            return Err(AppError::Conflict(format!(
                "tenant {} byte quota exceeded: {would_be} > {limit}",
                snap.tenant_id
            )));
        }
        atlas_inventory::tenants::QuotaCheck::Count { limit, current } => {
            return Err(AppError::Conflict(format!(
                "tenant {} volume-count quota exceeded: {current} already at limit {limit}",
                snap.tenant_id
            )));
        }
    }

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
