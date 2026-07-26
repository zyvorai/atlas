<!-- Copyright (c) 2026 ZyvorAI Labs Private Limited. All rights reserved. -->
# Atlas Documentation Index

Start at the top-level [README](../README.md) for the overview and quickstart.

## Guides
- **[GETTING_STARTED.md](GETTING_STARTED.md)** — build, run locally (fake driver), `atlasctl`, tests.
- **[ARCHITECTURE.md](ARCHITECTURE.md)** — control-plane design, driver model, data model, request flow, config.
- **[API.md](API.md)** — REST v1 reference with real request/response examples.
- **[DEPLOYMENT.md](DEPLOYMENT.md)** — k3s + Rook (Squid / CSI drivers) + gateway scripts; pitfalls table.
- **[DAY2.md](DAY2.md)** — day-2 ops (alerts, maintenance, governance, DR).
- **[DR.md](DR.md)** — cross-cluster RBD mirroring, failover runbook, live two-site checklist.
- **[HA.md](HA.md)** — durable job queue, leader lease, Postgres cutover plan.
- **[DATABRIDGE.md](DATABRIDGE.md)** — cloud-to-edge DB / object migration control plane.
- **[ROADMAP.md](ROADMAP.md)** — what's shipped and what's deferred.
- **[../CONTRIBUTING.md](../CONTRIBUTING.md)** — conventions; how to add an endpoint / driver / migration.

## Deploy assets
- **[DEPLOYMENT.md](DEPLOYMENT.md)** — end-to-end k3s deploy; version lockstep + pitfalls.
- **[../deploy/rook-ceph-lab/README.md](../deploy/rook-ceph-lab/README.md)** — Rook Ceph + KubeVirt/CDI lab (`up.sh --single-node`).
- `../deploy/rook-ceph-lab/single-node/` — single-OSD overlay for a one-node k3s.
- `../deploy/k8s/` — gateway Deployment/RBAC/Service (fake + real ceph variants).
- **[../scripts/README.md](../scripts/README.md)** — `deploy-remote.sh` / `deploy-ceph-gateway-remote.sh`.

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
