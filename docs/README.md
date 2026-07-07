<!-- Copyright (c) 2026 ZyvorAI Labs Private Limited. All rights reserved. -->
# Atlas Documentation Index

Start at the top-level [README](../README.md) for the overview and quickstart.

## Guides
- **[GETTING_STARTED.md](GETTING_STARTED.md)** — build, run locally (fake driver), `atlasctl`, tests.
- **[ARCHITECTURE.md](ARCHITECTURE.md)** — control-plane design, driver model, data model, request flow, config.
- **[API.md](API.md)** — REST v1 reference with real request/response examples.
- **[DEPLOYMENT.md](DEPLOYMENT.md)** — deploy to k3s, `deploy-remote.sh`, real Ceph mode end-to-end.
- **[ROADMAP.md](ROADMAP.md)** — what's done (slice 1) and what's next (slices 2/3+).
- **[../CONTRIBUTING.md](../CONTRIBUTING.md)** — conventions; how to add an endpoint / driver / migration.

## Deploy assets
- **[../deploy/rook-ceph-lab/README.md](../deploy/rook-ceph-lab/README.md)** — Rook Ceph + KubeVirt/CDI lab.
- `../deploy/rook-ceph-lab/single-node/` — single-OSD overlay for a one-node k3s.
- `../deploy/k8s/` — gateway Deployment/RBAC/Service (fake + real ceph variants).

## Per-crate docs
Each crate has its own `README.md`:

| Crate | Role |
|---|---|
| [`atlas-common`](../crates/atlas-common/README.md) | config, error type, tracing, id helpers |
| [`atlas-api-types`](../crates/atlas-api-types/README.md) | shared serde DTOs (the contract) |
| [`atlas-driver-core`](../crates/atlas-driver-core/README.md) | `StorageDriver` trait + registry |
| [`atlas-driver-ceph`](../crates/atlas-driver-ceph/README.md) | real + fake Ceph drivers |
| [`atlas-driver-k8s`](../crates/atlas-driver-k8s/README.md) | live Kubernetes driver |
| [`atlas-inventory`](../crates/atlas-inventory/README.md) | SQLite read/upsert model + audit |
| [`atlas-discovery`](../crates/atlas-discovery/README.md) | discovery worker |
| [`atlas-gateway`](../crates/atlas-gateway/README.md) | axum server (bin) |
| [`atlas-cli`](../crates/atlas-cli/README.md) | `atlasctl` REST client |

## Design authority
`Zyvor_Ceph_Integration_Developer_Implementation_Plan.pdf` — section refs (e.g. "PDF §10.2")
throughout the code/docs point back to it.
