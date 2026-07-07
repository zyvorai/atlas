<!-- Copyright (c) 2026 ZyvorAI Labs Private Limited. All rights reserved. -->
# Getting Started (local development)

## Prerequisites

- Rust (stable; the CI/Docker builds pin `1.88`, any recent stable works locally).
- No database server needed — Atlas uses **SQLite** (file created automatically).
- No Kubernetes/Ceph needed for the fake path.

## Build

```bash
cargo build --workspace          # or: make build
```

## Run the gateway (fake Ceph driver — no cluster required)

```bash
make run
# equivalent to:
ATLAS_CEPH_DRIVER_MODE=fake ATLAS_BIND_ADDR=127.0.0.1:5110 \
  cargo run -p atlas-gateway
```

On startup the gateway:
1. opens/creates the SQLite DB and runs migrations,
2. registers the Ceph backend (`bkd_ceph_lab`) + its driver,
3. attaches the live Kubernetes driver **if** a cluster is reachable (else logs a warning),
4. runs one discovery pass so inventory is populated immediately.

## Talk to it with `atlasctl`

```bash
cargo run -p atlas-cli -- health
cargo run -p atlas-cli -- discover
cargo run -p atlas-cli -- clusters
cargo run -p atlas-cli -- pools
cargo run -p atlas-cli -- volumes
cargo run -p atlas-cli -- metrics
```

Or with `curl`:

```bash
curl -s localhost:5110/health
curl -s -X POST localhost:5110/api/atlas/v1/backends/bkd_ceph_lab/discover
curl -s localhost:5110/api/atlas/v1/pools | jq
```

Expected fake inventory: 3 pools (`rbd-nvme-prod`, `cephfs-data0`, `.rgw.root`),
2 volumes, 6 OSDs.

## Point the live k8s driver at a cluster

Set `KUBECONFIG` (or `ATLAS_KUBECONFIG`) and the `/storage-classes`, `/kubernetes/pvcs`,
`/kubernetes/pvs` endpoints return **real** cluster objects:

```bash
KUBECONFIG=~/.kube/config make run
cargo run -p atlas-cli -- storage-classes
```

## Run the tests

```bash
cargo test --workspace          # or: make test
```

What's covered:
- **Unit**: id helpers, Ceph pool-kind/health mapping, Ceph-provisioner detection.
- **Integration** (`crates/atlas-gateway/tests/read_only.rs`): spins up the gateway with the
  fake driver + a throwaway SQLite file and asserts discovery populates inventory, cluster
  health/capabilities resolve, `/metrics/summary` aggregates, audit rows are written, unknown
  volume → 404, and `/storage-classes` without a cluster → 502.

## Lint & format

```bash
make fmt          # cargo fmt --all
make lint         # cargo clippy --workspace --all-targets -- -D warnings
make dev          # fmt + lint + test
```

## Common env vars

See [.env.example](../.env.example) and the config table in
[ARCHITECTURE.md](ARCHITECTURE.md#configuration). Copy `.env.example` to `.env` to have it
loaded automatically (`dotenvy`).

## Switching to the real Ceph driver locally

`ATLAS_CEPH_DRIVER_MODE=real` makes the driver shell out to `ceph`/`rbd`. You need those CLIs
installed and a reachable cluster with `/etc/ceph/ceph.conf` + keyring. For a full real setup
see [DEPLOYMENT.md](DEPLOYMENT.md) (which runs it inside Kubernetes with the Rook keyring).
