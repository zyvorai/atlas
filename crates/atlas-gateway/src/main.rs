// Copyright (c) 2026 ZyvorAI Labs Private Limited. All rights reserved.
//! Atlas gateway binary: the REST edge of the storage control plane.

use std::net::SocketAddr;

use atlas_common::Config;
use atlas_gateway::routes;
use atlas_gateway::startup::{build_state, BuildOptions};
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;
use tracing::info;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    atlas_common::init_tracing();

    let config = Config::from_env();
    info!(?config, "starting atlas-gateway");
    if config.jwt_secret_is_weak() {
        tracing::warn!(
            "ATLAS_JWT_SECRET is the dev default or shorter than 32 bytes — set a strong secret in production"
        );
    }

    let bind_addr = config.bind_addr.clone();
    let state = build_state(config, BuildOptions::default()).await?;

    let app = routes::router(state)
        .layer(TraceLayer::new_for_http())
        .layer(CorsLayer::permissive());

    let addr: SocketAddr = bind_addr.parse()?;
    let listener = tokio::net::TcpListener::bind(addr).await?;
    info!("atlas-gateway listening on http://{addr}");
    axum::serve(listener, app).await?;
    Ok(())
}
