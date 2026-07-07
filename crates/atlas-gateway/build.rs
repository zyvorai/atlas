// Copyright (c) 2026 ZyvorAI Labs Private Limited. All rights reserved.
//! Compile the gRPC proto. Requires `protoc` (system package `protobuf-compiler`) at build time —
//! installed in the Docker builder stages and present on dev machines.
fn main() {
    println!("cargo:rerun-if-changed=proto/atlas.proto");
    let out = std::path::PathBuf::from(std::env::var("OUT_DIR").unwrap());
    tonic_build::configure()
        .file_descriptor_set_path(out.join("atlas_descriptor.bin"))
        .compile_protos(&["proto/atlas.proto"], &["proto"])
        .expect("compile atlas.proto");
}
