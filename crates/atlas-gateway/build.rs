// Copyright (c) 2026 ZyvorAI Labs Private Limited. All rights reserved.
//! Compile the gRPC proto. Requires `protoc` (system package `protobuf-compiler`) at build time —
//! installed in the Docker builder stages and present on dev machines.
fn main() {
    println!("cargo:rerun-if-changed=proto/atlas.proto");
    tonic_build::compile_protos("proto/atlas.proto").expect("compile atlas.proto");
}
