// Copyright (c) 2026 ZyvorAI Labs Private Limited. All rights reserved.
//! Trial/license enforcement. Verification and the wire status shape live in `atlas-license`;
//! this module owns the two things that are genuinely server-specific: locating *where* the
//! token is (env var vs file) and gating requests with it.
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

/// Checked in order: explicit env value, then an env-pointed file, then a default file path next
/// to the binary. Re-resolved on every call (not cached in `AppState`) so replacing the token
/// file takes effect without a restart — the check itself is one cheap Ed25519 verify.
fn locate_token() -> Option<String> {
    for var in ["ATLAS_LICENSE_KEY", "ATLAS_TRIAL_TOKEN"] {
        if let Ok(t) = std::env::var(var) {
            let t = t.trim().to_string();
            if !t.is_empty() {
                return Some(t);
            }
        }
    }
    if let Ok(path) = std::env::var("ATLAS_TRIAL_TOKEN_FILE") {
        if let Ok(s) = std::fs::read_to_string(&path) {
            let s = s.trim().to_string();
            if !s.is_empty() {
                return Some(s);
            }
        }
    }
    if let Ok(s) = std::fs::read_to_string("trial.token") {
        let s = s.trim().to_string();
        if !s.is_empty() {
            return Some(s);
        }
    }
    None
}

/// `GET /license/status` — always reachable, trial-expired or not (see `routes/mod.rs`'s
/// `public_api`).
pub async fn license_status() -> Json<atlas_license::LicenseStatus> {
    Json(atlas_license::status(locate_token().as_deref()))
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
    let token = locate_token();
    if !atlas_license::is_active(token.as_deref()) {
        return license_required(
            "no active Atlas trial or license found — see GET /license/status",
        );
    }
    next.run(req).await
}
