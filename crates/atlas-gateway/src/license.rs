// Copyright (c) 2026 ZyvorAI Labs Private Limited. All rights reserved.
//! Trial/license enforcement. Verification, the wire status shape, and token *location*
//! (`atlas_license::locate_token_from_env` — shared with `Config::validate_for_start`'s startup
//! warning) live in `atlas-license`; this module owns the one thing that's genuinely
//! server-specific: gating requests with it.
//!
//! Mirrors `auth_middleware`'s shape (`crate::auth`) — same `State<AppState>` signature, same
//! "config toggle decides whether this even runs" pattern (`auth_required` / `license_enforce`).

use axum::{
    body::Body,
    extract::State,
    http::{Request, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
    Json,
};
use serde_json::json;

use crate::state::AppState;

/// `GET /license/status` — always reachable, trial-expired or not (see `routes/mod.rs`'s
/// `public_api`).
pub async fn license_status() -> Json<atlas_license::LicenseStatus> {
    Json(atlas_license::status(
        atlas_license::locate_token_from_env().as_deref(),
    ))
}

fn license_required(msg: &str) -> Response {
    (
        StatusCode::PAYMENT_REQUIRED,
        Json(json!({
            "error": { "code": "LICENSE_REQUIRED", "message": msg },
            "contact": atlas_license::SALES_CONTACT,
            "trial_expired": true,
        })),
    )
        .into_response()
}

/// Gates every route it wraps behind an active trial/license token when
/// `config.license_enforce` is set (the default — see `Config::license_enforce`). Wrap only
/// bearer-protected product routes with this, the same way `auth_middleware` is applied — never
/// `/health`, `/license/status`, or the login routes, which must stay reachable so an expired
/// install can still show *why* it's gated.
pub async fn license_middleware(
    State(state): State<AppState>,
    req: Request<Body>,
    next: Next,
) -> Response {
    if !state.config.license_enforce {
        return next.run(req).await;
    }
    let token = atlas_license::locate_token_from_env();
    if !atlas_license::is_active(token.as_deref()) {
        return license_required(
            "no active Atlas trial or license found — see GET /license/status",
        );
    }
    next.run(req).await
}
