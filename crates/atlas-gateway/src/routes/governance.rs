// Copyright (c) 2026 ZyvorAI Labs Private Limited. All rights reserved.
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Extension, Json,
};
use serde::Deserialize;
use serde_json::{json, Value};

use atlas_common::{ids, AppError, AppResult};

use crate::auth::Actor;
use crate::state::AppState;
use super::object_store::ListBackupsQuery;

// ---- policies ----

pub(crate) async fn list_policies() -> Json<Value> {
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
pub(crate) struct ScheduleBody {
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
pub(crate) async fn create_schedule(
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
    let vol = atlas_inventory::get_volume(&s.pool, &id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("volume {id}")))?;
    // Both snapshot and backup schedules dispatch a CSI VolumeSnapshot job under the hood, which
    // needs a PVC-backed (k8s) volume. Without this check a schedule against a raw NFS/ZFS volume
    // creates successfully and its "next run" timer keeps advancing forever, but no job is ever
    // enqueued for it — a silent, permanent no-op that looks like a healthy active schedule.
    if vol.pvc_name.is_none() {
        return Err(AppError::Validation(format!(
            "volume {id} has no PVC — schedules require a PVC-backed (CSI) volume"
        )));
    }
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
pub(crate) async fn list_schedules(
    State(s): State<AppState>,
    Query(q): Query<ListBackupsQuery>,
) -> AppResult<Json<Value>> {
    let items = atlas_inventory::schedules::list(&s.pool, q.volume_id.as_deref()).await?;
    Ok(Json(json!(items)))
}

/// `DELETE /schedules/{id}` (operator).
pub(crate) async fn delete_schedule(
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
pub(crate) struct IssueTokenBody {
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
pub(crate) async fn issue_token(
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
    let (token, exp, jti) =
        crate::auth::mint_token(&s.config.jwt_secret, &body.subject, &role, ttl_secs)?;
    let _ = atlas_inventory::audit::record(
        &s.pool,
        None,
        &actor.id,
        "auth.token.issued",
        "service_account",
        &body.subject,
        "ok",
        Some(json!({ "role": role, "ttl_secs": ttl_secs, "jti": jti })),
        None,
    )
    .await;
    Ok((
        StatusCode::CREATED,
        Json(json!({
            "token": token, "jti": jti, "subject": body.subject, "role": role,
            "level": crate::auth::role_level(&role), "expires_at": exp, "ttl_secs": ttl_secs
        })),
    ))
}

/// `GET /auth/tokens/revoked` — the current token deny-list (admin).
pub(crate) async fn list_revoked_tokens(
    State(s): State<AppState>,
    Extension(actor): Extension<Actor>,
) -> AppResult<Json<Value>> {
    crate::auth::require_role(s.config.auth_required, &actor, crate::auth::ROLE_ADMIN)?;
    Ok(Json(json!(atlas_inventory::tokens::list(&s.pool).await?)))
}

/// `POST /auth/tokens/{jti}/revoke` — kill a minted token by its id before it expires (admin).
pub(crate) async fn revoke_token(
    State(s): State<AppState>,
    Extension(actor): Extension<Actor>,
    Path(jti): Path<String>,
) -> AppResult<Json<Value>> {
    crate::auth::require_role(s.config.auth_required, &actor, crate::auth::ROLE_ADMIN)?;
    atlas_inventory::tokens::revoke(&s.pool, &jti, &actor.id).await?;
    let _ = atlas_inventory::audit::record(
        &s.pool, None, &actor.id, "auth.token.revoked", "service_account", &jti, "ok", None, None,
    )
    .await;
    Ok(Json(json!({ "jti": jti, "revoked": true })))
}

pub(crate) async fn list_volume_bindings(
    State(s): State<AppState>,
    Path(id): Path<String>,
) -> AppResult<Json<Value>> {
    let rows = atlas_inventory::list_bindings_for(&s.pool, "volume", &id).await?;
    Ok(Json(json!(rows)))
}

/// `GET /volumes/{id}/labels` — the volume's user labels.
pub(crate) async fn get_volume_labels(
    State(s): State<AppState>,
    Path(id): Path<String>,
) -> AppResult<Json<Value>> {
    Ok(Json(
        atlas_inventory::get_volume_labels(&s.pool, &id).await?,
    ))
}

/// `PUT /volumes/{id}/labels` — merge labels into the volume (operator). Body is a JSON object.
pub(crate) async fn put_volume_labels(
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
pub(crate) async fn list_tenants(State(s): State<AppState>) -> AppResult<Json<Value>> {
    Ok(Json(json!(
        atlas_inventory::tenants::list_overview(&s.pool).await?
    )))
}
pub(crate) async fn list_tenant_policies(
    State(s): State<AppState>,
    Path(id): Path<String>,
) -> AppResult<Json<Value>> {
    let items = atlas_inventory::tenants::list_policies(&s.pool, &id).await?;
    Ok(Json(json!(items)))
}

#[derive(Debug, Deserialize)]
pub(crate) struct TenantPolicyBody {
    storage_class: String,
    #[serde(default)]
    access_mode: Option<String>,
    #[serde(default)]
    volume_mode: Option<String>,
}

/// `PUT /tenants/{id}/policies/{intent}` — override an intent's placement for a tenant (admin).
pub(crate) async fn put_tenant_policy(
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
pub(crate) async fn delete_tenant_policy(
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
pub(crate) async fn get_tenant_quota(
    State(s): State<AppState>,
    Path(id): Path<String>,
) -> AppResult<Json<Value>> {
    let q = atlas_inventory::tenants::get_quota(&s.pool, &id).await?;
    Ok(Json(json!(q)))
}

#[derive(Debug, Deserialize)]
pub(crate) struct TenantQuotaBody {
    /// Max total provisioned volume bytes (0 = unlimited).
    #[serde(default)]
    max_bytes: i64,
    /// Max number of volumes (0 = unlimited).
    #[serde(default)]
    max_volumes: i64,
}

/// `PUT /tenants/{id}/quota` — set the tenant's quota (admin).
pub(crate) async fn put_tenant_quota(
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
