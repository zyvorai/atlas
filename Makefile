# Copyright (c) 2026 ZyvorAI Labs Private Limited. All rights reserved.
.PHONY: dev build release test lint fmt fmt-check run cli clean

dev: fmt lint test

build:
	cargo build --workspace

release:
	cargo build --workspace --release

test:
	cargo test --workspace

lint:
	cargo clippy --workspace --all-targets -- -D warnings

fmt:
	cargo fmt --all

fmt-check:
	cargo fmt --all --check

# Run the gateway with the fake Ceph driver (no cluster required).
run:
	ATLAS_CEPH_DRIVER_MODE=fake cargo run -p atlas-gateway

cli:
	cargo run -p atlas-cli --

clean:
	cargo clean
	rm -f atlas.db atlas.db-wal atlas.db-shm
