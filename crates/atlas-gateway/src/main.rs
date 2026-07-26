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
    if let Err(msg) = config.validate_for_start() {
        anyhow::bail!("{msg}");
    }
    info!(?config, "starting atlas-gateway");
    if config.jwt_secret_is_weak() {
        tracing::warn!(
            "ATLAS_JWT_SECRET is the dev default or shorter than 32 bytes — set a strong secret in production"
        );
    }
    if config.auth_required && config.bootstrap_admin_token.is_some() {
        tracing::warn!(
            "ATLAS_BOOTSTRAP_ADMIN_TOKEN is set — use it once to mint service-account JWTs, then remove it from the Secret"
        );
    }

    let bind_addr = config.bind_addr.clone();
    let grpc_addr = config.grpc_addr.clone();
    let https_addr = config.https_addr.clone();
    let tls_cert = config.tls_cert_path.clone();
    let tls_key = config.tls_key_path.clone();
    let state = build_state(config, BuildOptions::default()).await?;

    // REST server (HTTP).
    let app = routes::router(state.clone())
        .layer(TraceLayer::new_for_http())
        .layer(CorsLayer::permissive());
    let addr: SocketAddr = bind_addr.parse()?;
    let listener = tokio::net::TcpListener::bind(addr).await?;
    info!("atlas-gateway REST listening on http://{addr}");
    let rest_app = app.clone();
    let rest = async move {
        axum::serve(listener, rest_app)
            .with_graceful_shutdown(shutdown_signal())
            .await
            .map_err(anyhow::Error::from)
    };

    // Optional HTTPS listener (same app), enabled by ATLAS_HTTPS_ADDR + cert/key PEM paths.
    if let (Some(haddr), Some(cert), Some(key)) = (https_addr, tls_cert, tls_key) {
        let _ = rustls::crypto::ring::default_provider().install_default();
        let tls = axum_server::tls_rustls::RustlsConfig::from_pem_file(&cert, &key)
            .await
            .map_err(|e| anyhow::anyhow!("load TLS cert/key: {e}"))?;
        let saddr: SocketAddr = haddr.parse()?;
        info!("atlas-gateway REST (TLS) listening on https://{saddr}");
        let https = async move {
            axum_server::bind_rustls(saddr, tls)
                .serve(app.into_make_service())
                .await
                .map_err(anyhow::Error::from)
        };
        tokio::spawn(async move {
            if let Err(e) = https.await {
                tracing::error!("https server error: {e:#}");
            }
        });
    }

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
            .serve_with_shutdown(gaddr, shutdown_signal())
            .await
            .map_err(anyhow::Error::from)
    };

    tokio::try_join!(rest, grpc)?;
    info!("atlas-gateway shut down cleanly");
    Ok(())
}

/// Resolves when the process receives SIGINT (Ctrl-C) or SIGTERM (k8s rollout/`docker stop`). Both
/// server tasks await their own copy so in-flight requests drain instead of being killed mid-flight.
/// An interrupted background job is recovered (failed-safe + re-enqueued) on the next start.
async fn shutdown_signal() {
    let ctrl_c = async {
        let _ = tokio::signal::ctrl_c().await;
    };
    #[cfg(unix)]
    let terminate = async {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(mut s) => {
                s.recv().await;
            }
            Err(_) => std::future::pending::<()>().await,
        }
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {}
        _ = terminate => {}
    }
    info!("shutdown signal received — draining in-flight requests");
}
