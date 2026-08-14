# Copyright (c) 2026 ZyvorAI Labs Private Limited. All rights reserved.
.PHONY: dev build release test lint fmt fmt-check run run-databridge cli clean ui ui-dev \
	features docker-smoke ci audit

dev: lint test

build:
	cargo build --workspace

release:
	cargo build --workspace --release

test:
	cargo test --workspace

lint:
	cargo clippy --workspace --all-targets -- -D warnings

# Supply-chain gate: known-vulnerable/yanked advisories, disallowed licenses, unknown sources
# (deny.toml). Installs cargo-deny on first run if missing.
audit:
	@command -v cargo-deny >/dev/null || cargo install cargo-deny --locked
	cargo deny check advisories bans licenses sources

fmt:
	cargo fmt --all

fmt-check:
	cargo fmt --all --check

# Compile-check optional DataBridge connectors (mirrors the CI feature gate).
features:
	cargo check -p atlas-databridge --features sqlserver
	cargo check -p atlas-databridge --features mongodb
	cargo check -p atlas-databridge --features azure-blob
	cargo check -p atlas-databridge --features oracle
	@command -v cmake >/dev/null && cargo check -p atlas-databridge --features kafka-lag \
		|| echo "skip kafka-lag (cmake not installed)"

# Fast Docker smoke: UI stages of both images (needs podman or docker).
docker-smoke:
	@RT=$$(command -v podman >/dev/null && echo podman || echo docker); \
	  $$RT build --target ui -t atlas-gateway:ui -f Dockerfile .; \
	  $$RT build --target ui -t atlas-gateway-ceph:ui -f Dockerfile.ceph .

# Local equivalent of the CI static gate (no Docker, no containers).
ci: lint test audit features ui

# Run the gateway with the fake Ceph driver (no cluster required).
run:
	ATLAS_CEPH_DRIVER_MODE=fake cargo run -p atlas-gateway

# Run the gateway for a DataBridge demo: fake Ceph + the reconciler ticking every 5s so fake CDC
# lag drains and the migration pipeline runs end-to-end with no cloud/k8s. See docs/DATABRIDGE.md.
run-databridge:
	ATLAS_CEPH_DRIVER_MODE=fake ATLAS_DATABRIDGE_RECONCILE_SECS=5 cargo run -p atlas-gateway

# Build the React Storage Center UI into crates/atlas-gateway/ui/dist (embedded by the gateway).
ui:
	cd crates/atlas-gateway/ui && npm ci && npm run build

# Vite dev server (hot reload) proxying /api to a running gateway (make run in another shell).
ui-dev:
	cd crates/atlas-gateway/ui && npm install && npm run dev

cli:
	cargo run -p atlas-cli --

clean:
	cargo clean
	rm -f atlas.db atlas.db-wal atlas.db-shm
