// Copyright (c) 2026 ZyvorAI Labs Private Limited. All rights reserved.
//! Atlas gateway library: state, auth, routes, and startup wiring shared by the binary and tests.

pub mod auth;
pub mod grpc;
pub mod license;
pub mod routes;
pub mod startup;
pub mod state;

pub use state::AppState;

/// Generated gRPC types + service stubs (package `atlas.v1`).
pub mod proto {
    tonic::include_proto!("atlas.v1");

    /// Encoded FileDescriptorSet for gRPC server reflection (lets grpcurl work without the proto).
    pub const FILE_DESCRIPTOR_SET: &[u8] =
        include_bytes!(concat!(env!("OUT_DIR"), "/atlas_descriptor.bin"));
}
