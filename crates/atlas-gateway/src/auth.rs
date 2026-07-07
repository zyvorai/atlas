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
use jsonwebtoken::{decode, Algorithm, DecodingKey, Validation};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::state::AppState;

/// JWT claims. `sub` is the actor id used in audit logs; `role` gates future write actions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Claims {
    pub sub: String,
    #[serde(default)]
    pub role: String,
    #[serde(default)]
    pub exp: usize,
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

    /// Actor id used for gRPC-originated actions (auth on the gRPC edge is a follow-up).
    pub fn grpc_id() -> &'static str {
        "grpc"
    }
}

pub async fn auth_middleware(
    State(state): State<AppState>,
    mut req: Request<Body>,
    next: Next,
) -> Response {
    if !state.config.auth_required {
        req.extensions_mut().insert(Actor::anonymous());
        return next.run(req).await;
    }

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
            req.extensions_mut().insert(Actor {
                id: data.claims.sub,
                role: data.claims.role,
            });
            next.run(req).await
        }
        Err(e) => unauthorized(&format!("invalid token: {e}")),
    }
}

fn unauthorized(msg: &str) -> Response {
    (
        StatusCode::UNAUTHORIZED,
        Json(json!({ "error": { "code": "AUTH_ERROR", "message": msg } })),
    )
        .into_response()
}
