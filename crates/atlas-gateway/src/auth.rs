// Copyright (c) 2026 ZyvorAI Labs Private Limited. All rights reserved.
//! JWT auth middleware. When `config.auth_required` is false (dev default), requests pass through
//! with an anonymous actor; when true, a valid HS256 Bearer token is required.

use axum::{
    body::Body,
    extract::State,
    http::{Request, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
    Json,
};
use jsonwebtoken::{decode, encode, Algorithm, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::state::AppState;

/// JWT claims. `sub` is the actor id used in audit logs; `role` gates write actions; `jti` is the
/// token id used for revocation (empty on legacy tokens, which are simply not revocable).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Claims {
    pub sub: String,
    #[serde(default)]
    pub role: String,
    #[serde(default)]
    pub exp: usize,
    #[serde(default)]
    pub jti: String,
}

/// The authenticated actor, injected as a request extension.
#[derive(Debug, Clone)]
pub struct Actor {
    pub id: String,
    pub role: String,
}

impl Actor {
    fn anonymous() -> Self {
        Self {
            id: "anonymous".into(),
            role: "viewer".into(),
        }
    }

    pub fn level(&self) -> u8 {
        role_level(&self.role)
    }
}

/// Role levels (PDF §14.2). Higher = more privilege; each level includes the ones below it.
pub const ROLE_VIEWER: u8 = 0;
pub const ROLE_OPERATOR: u8 = 1;
pub const ROLE_ADMIN: u8 = 2;

/// Map a JWT `role` claim to a privilege level. Product service accounts get operator-level access
/// for volume lifecycle. Unknown roles are viewer (read-only).
pub fn role_level(role: &str) -> u8 {
    match role {
        "admin" | "storage.admin" | "storage.security" | "storage.breakglass" => ROLE_ADMIN,
        "operator" | "storage.operator" => ROLE_OPERATOR,
        r if r.starts_with("product.service.") => ROLE_OPERATOR,
        _ => ROLE_VIEWER,
    }
}

/// Mint a signed HS256 service-account token for `subject` with `role`, expiring in `ttl_secs`.
/// Returns `(token, exp_unix_secs, jti)`. The `jti` identifies the token for later revocation. Used
/// by the token-issuance endpoint so products (Veyron, Hyper2KVM, …) get least-privilege credentials
/// without the shared secret ever leaving Atlas.
pub fn mint_token(
    secret: &str,
    subject: &str,
    role: &str,
    ttl_secs: u64,
) -> atlas_common::AppResult<(String, usize, String)> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|e| atlas_common::AppError::Internal(e.to_string()))?
        .as_secs();
    let exp = (now + ttl_secs) as usize;
    let jti = atlas_common::ids::token_jti();
    let claims = Claims {
        sub: subject.to_string(),
        role: role.to_string(),
        exp,
        jti: jti.clone(),
    };
    let token = encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(secret.as_bytes()),
    )
    .map_err(|e| atlas_common::AppError::Internal(format!("mint token: {e}")))?;
    Ok((token, exp, jti))
}

/// Enforce a minimum role. No-op when auth is disabled (dev), so open-dev keeps working.
pub fn require_role(auth_required: bool, actor: &Actor, min: u8) -> atlas_common::AppResult<()> {
    if auth_required && actor.level() < min {
        return Err(atlas_common::AppError::Forbidden(format!(
            "action requires a higher role; actor '{}' has role '{}'",
            actor.id, actor.role
        )));
    }
    Ok(())
}

pub async fn auth_middleware(
    State(state): State<AppState>,
    mut req: Request<Body>,
    next: Next,
) -> Response {
    // Resolve the actor: anonymous when auth is disabled, else a valid non-revoked Bearer token.
    let actor = if !state.config.auth_required {
        Actor::anonymous()
    } else {
        let token = req
            .headers()
            .get(http::header::AUTHORIZATION)
            .and_then(|h| h.to_str().ok())
            .and_then(|s| s.strip_prefix("Bearer "));
        let Some(token) = token else {
            return unauthorized("missing bearer token");
        };
        let mut validation = Validation::new(Algorithm::HS256);
        validation.validate_exp = true;
        match decode::<Claims>(
            token,
            &DecodingKey::from_secret(state.config.jwt_secret.as_bytes()),
            &validation,
        ) {
            Ok(data) => {
                // Deny-list check: a revoked token is rejected even before it expires. Fail open on a
                // DB error (readiness already gates on the DB) so a transient blip can't lock everyone out.
                if !data.claims.jti.is_empty() {
                    match atlas_inventory::tokens::is_revoked(&state.pool, &data.claims.jti).await {
                        Ok(true) => return unauthorized("token has been revoked"),
                        Ok(false) => {}
                        Err(e) => tracing::warn!("token revocation check failed: {e}"),
                    }
                }
                Actor { id: data.claims.sub, role: data.claims.role }
            }
            Err(e) => return unauthorized(&format!("invalid token: {e}")),
        }
    };

    // Rate limit per actor (day-2 governance; disabled unless ATLAS_RATE_LIMIT_RPM > 0).
    if !state.rate.allow(&actor.id) {
        return too_many_requests(&actor.id);
    }

    req.extensions_mut().insert(actor);
    next.run(req).await
}

fn unauthorized(msg: &str) -> Response {
    (
        StatusCode::UNAUTHORIZED,
        Json(json!({ "error": { "code": "AUTH_ERROR", "message": msg } })),
    )
        .into_response()
}

fn too_many_requests(actor: &str) -> Response {
    (
        StatusCode::TOO_MANY_REQUESTS,
        Json(json!({ "error": { "code": "RATE_LIMITED", "message": format!("rate limit exceeded for '{actor}'") } })),
    )
        .into_response()
}
