// Copyright (c) 2026 ZyvorAI Labs Private Limited. All rights reserved.
//! Atlas gateway binary: REST + gRPC edges of the storage control plane.

use std::net::SocketAddr;

use atlas_common::Config;
use atlas_gateway::startup::{build_state, BuildOptions};
use atlas_gateway::{grpc, routes};
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
    let grpc_addr = config.grpc_addr.clone();
    let state = build_state(config, BuildOptions::default()).await?;

    // REST server.
    let app = routes::router(state.clone())
        .layer(TraceLayer::new_for_http())
        .layer(CorsLayer::permissive());
    let addr: SocketAddr = bind_addr.parse()?;
    let listener = tokio::net::TcpListener::bind(addr).await?;
    info!("atlas-gateway REST listening on http://{addr}");
    let rest = async move {
        axum::serve(listener, app)
            .await
            .map_err(anyhow::Error::from)
    };

    // gRPC edge (served concurrently). Empty ATLAS_GRPC_ADDR disables it.
    if grpc_addr.trim().is_empty() {
        rest.await?;
        return Ok(());
    }
    let gaddr: SocketAddr = grpc_addr.parse()?;
    info!("atlas-gateway gRPC listening on {gaddr}");
    let reflection = tonic_reflection::server::Builder::configure()
        .register_encoded_file_descriptor_set(atlas_gateway::proto::FILE_DESCRIPTOR_SET)
        .build_v1()?;
    let grpc = async move {
        tonic::transport::Server::builder()
            .add_service(grpc::service(state))
            .add_service(reflection)
            .serve(gaddr)
            .await
            .map_err(anyhow::Error::from)
    };

    tokio::try_join!(rest, grpc)?;
    Ok(())
}
