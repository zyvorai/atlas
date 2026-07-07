// Copyright (c) 2026 ZyvorAI Labs Private Limited. All rights reserved.
//! Atlas gateway library: state, auth, routes, and startup wiring shared by the binary and tests.

pub mod auth;
pub mod grpc;
pub mod routes;
pub mod startup;
pub mod state;

pub use state::AppState;

/// Generated gRPC types + service stubs (package `atlas.v1`).
pub mod proto {
    tonic::include_proto!("atlas.v1");
}
