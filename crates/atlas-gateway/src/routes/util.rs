// Copyright (c) 2026 ZyvorAI Labs Private Limited. All rights reserved.

use axum::{http::StatusCode, Json};
use atlas_api_types::{BackendType, Capabilities};
use serde_json::{json, Value};

pub(crate) const CEPH_BACKEND_ID: &str = "bkd_ceph_lab";
pub(crate) const DEFAULT_RBD_POOL: &str = "rbd-nvme-prod";

pub(crate) fn accepted(job: &atlas_api_types::JobRecord, resource: Value) -> (StatusCode, Json<Value>) {
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

pub(crate) fn csv_field(v: &Value, key: &str) -> String {
    let s = v.get(key).and_then(|x| x.as_str()).unwrap_or("");
    // Neutralize CSV formula injection: a cell starting with =/+/-/@ can be interpreted as a
    // formula by spreadsheet apps (Excel/Sheets) when this export is opened.
    let needs_guard = matches!(s.as_bytes().first(), Some(b'=' | b'+' | b'-' | b'@'));
    let guarded = if needs_guard { format!("'{s}") } else { s.to_string() };
    format!("\"{}\"", guarded.replace('"', "\"\""))
}

pub(crate) fn ceph_default_caps(t: BackendType) -> Capabilities {
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
