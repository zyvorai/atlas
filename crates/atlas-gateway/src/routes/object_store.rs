// Copyright (c) 2026 ZyvorAI Labs Private Limited. All rights reserved.
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Extension, Json,
};
use serde::Deserialize;
use serde_json::{json, Value};

use atlas_common::{ids, AppError, AppResult};
use atlas_jobs::JobSpec;

use crate::auth::Actor;
use crate::state::AppState;
use super::util::{accepted, CEPH_BACKEND_ID};
use super::volumes::ForceParams;

// ---- object storage (buckets) ----

pub(crate) async fn list_buckets(State(s): State<AppState>) -> AppResult<Json<Value>> {
    Ok(Json(json!(
        atlas_inventory::buckets::list_buckets(&s.pool).await?
    )))
}

pub(crate) async fn get_bucket(State(s): State<AppState>, Path(id): Path<String>) -> AppResult<Json<Value>> {
    let b = atlas_inventory::buckets::get_bucket(&s.pool, &id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("bucket {id}")))?;
    Ok(Json(json!(b)))
}

/// `GET /buckets/{id}/stats` — RGW usage + quota for the bucket (via `radosgw-admin bucket stats`).
pub(crate) async fn bucket_stats(State(s): State<AppState>, Path(id): Path<String>) -> AppResult<Json<Value>> {
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
pub(crate) struct PrefixQuery {
    prefix: Option<String>,
}

#[derive(serde::Deserialize)]
pub(crate) struct ObjectKeyQuery {
    key: Option<String>,
    ttl_secs: Option<u64>,
}

/// Build an `S3Target` for a bound bucket, reading the OBC credentials from its in-cluster
/// Secret. The endpoint prefers `rgw_public_endpoint` (the browser-reachable URL) so presigned
/// upload/download URLs it mints are usable from outside the cluster. Shared by every object op.
pub(crate) async fn bucket_s3_target(
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
pub(crate) async fn bucket_objects(
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
pub(crate) async fn bucket_object_upload_url(
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
pub(crate) async fn bucket_object_download_url(
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
pub(crate) async fn bucket_object_delete(
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
pub(crate) async fn bucket_objects_prune(
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
pub(crate) async fn delete_bucket(
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
pub(crate) struct CreateBucketBody {
    name: String,
    namespace: Option<String>,
    storage_class: Option<String>,
    /// Optional RGW quota: max object count.
    max_objects: Option<i64>,
    /// Optional RGW quota: max size (e.g. "2G").
    max_size: Option<String>,
}

/// `POST /buckets` — provision an RGW bucket via an ObjectBucketClaim (async job).
pub(crate) async fn create_bucket(
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
pub(crate) struct ListBackupsQuery {
    pub(crate) volume_id: Option<String>,
}

pub(crate) async fn list_backups(
    State(s): State<AppState>,
    Query(q): Query<ListBackupsQuery>,
) -> AppResult<Json<Value>> {
    Ok(Json(json!(
        atlas_inventory::backups::list_backups(&s.pool, q.volume_id.as_deref()).await?
    )))
}

pub(crate) async fn get_backup(State(s): State<AppState>, Path(id): Path<String>) -> AppResult<Json<Value>> {
    let b = atlas_inventory::backups::get_backup(&s.pool, &id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("backup {id}")))?;
    Ok(Json(json!(b)))
}

/// `DELETE /backups/{id}` — remove the backup's S3 objects + RBD snapshot + row (async job).
pub(crate) async fn delete_backup(
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
pub(crate) struct DownloadQuery {
    /// "manifest" (default) or "data" (the `.rbd-diff` object).
    what: Option<String>,
}

/// `GET /backups/{id}/download?what=data|manifest` — a time-limited presigned S3 URL for the
/// backup object, so a client downloads it straight from RGW (no proxy, no credentials).
pub(crate) async fn download_backup(
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
pub(crate) fn make_backup_delete_spec(
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
pub(crate) async fn enqueue_backup_delete(s: &AppState, actor_id: &str, old: &atlas_api_types::BackupRecord) {
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
pub(crate) async fn prune_backups(s: &AppState, actor_id: &str, volume_id: &str, keep: i64) {
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
pub(crate) async fn prune_backups_by_age(s: &AppState, actor_id: &str, volume_id: &str, max_age_secs: i64) {
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
pub(crate) struct CreateBackupBody {
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
pub(crate) async fn create_backup(
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
pub(crate) struct CreateRestoreBody {
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
pub(crate) async fn create_restore(
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
