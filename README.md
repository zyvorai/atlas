<!-- Copyright (c) 2026 ZyvorAI Labs Private Limited. All rights reserved. -->
# Atlas — Zyvor Storage Control Plane

Atlas is the **central storage control plane** for the Zyvor product suite. Products
(Zeus OS, Veyron, Hyper2KVM, GuestKit, PacketWolf, Aether, Ragnarok, Machina, HyperSDK)
call **stable Atlas APIs**; Atlas talks to storage backends through **pluggable drivers**.
**Ceph is the first driver** (RBD block, CephFS file, RGW/S3 object).

> Core principle: products request *intent* ("give me production block storage"), not pool
> internals. Atlas maps intent → backend, owns inventory/ownership/audit, and keeps every
> product decoupled from Ceph (or any future NFS/ZFS/SAN/cloud backend).

```
 Zeus OS · Veyron · Hyper2KVM · GuestKit · PacketWolf · Aether · Ragnarok · Machina · HyperSDK
                                        │  (REST / gRPC)
                                        ▼
                              ┌──────────────────┐
                              │  Atlas Gateway   │  auth · audit · API
                              └────────┬─────────┘
                     ┌─────────────────┼──────────────────┐
                     ▼                 ▼                  ▼
                 discovery         inventory           drivers
                 (normalize)      (SQLite RO)     ┌──────┴───────┐
                                                  │ Ceph  │  K8s  │
                                                  │ (RBD/ │ (SC/  │
                                                  │ CephFS│  PVC/ │
                                                  │ /RGW) │  PV)  │
                                                  └───────┴───────┘
                                             Ceph cluster   Kubernetes
```

## Status — MVP slice 1 (read-only backend + Rook lab) ✅

Implemented and **verified end-to-end on a real k3s + Rook Ceph cluster**:

- `atlas-*` Cargo workspace; axum 0.8 gateway; SQLite (`sqlx`) inventory.
- **Read-only** REST discovery/inventory API (`/api/atlas/v1/...`).
- Pluggable `StorageDriver` trait with:
  - a **real Ceph driver** (`ceph`/`rbd` CLI, arg-arrays only), and a **fake driver** (fixtures);
  - a **live Kubernetes driver** (lists StorageClasses / PVCs / PVs via `kube-rs`).
- `atlas-discovery` worker normalizing driver output into inventory.
- `deploy/rook-ceph-lab/` manifests + `up.sh` to stand up Rook Ceph + KubeVirt/CDI.
- `deploy/k8s/` + `scripts/deploy-remote.sh` to deploy the gateway onto a k3s node.

**Deferred to later slices:** write/provisioning path + job engine, gRPC edge, RGW/backup,
per-product integrations, Zeus OS UI. See [docs/ROADMAP.md](docs/ROADMAP.md).

## Quickstart (no Ceph, no cluster needed)

```bash
# 1. Run the gateway with the fake Ceph driver
make run                       # ATLAS_CEPH_DRIVER_MODE=fake on 127.0.0.1:5110

# 2. Talk to it
cargo run -p atlas-cli -- health
cargo run -p atlas-cli -- discover      # populate inventory from the fake driver
cargo run -p atlas-cli -- pools
cargo run -p atlas-cli -- volumes
```

Full local walkthrough: **[docs/GETTING_STARTED.md](docs/GETTING_STARTED.md)**.

## Documentation

| Doc | What's in it |
|---|---|
| [docs/GETTING_STARTED.md](docs/GETTING_STARTED.md) | Build, run locally, `atlasctl`, run the tests |
| [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) | Control-plane design, driver model, data model, request flow |
| [docs/API.md](docs/API.md) | REST API reference with request/response examples |
| [docs/DEPLOYMENT.md](docs/DEPLOYMENT.md) | Deploy to k3s, `deploy-remote.sh`, real Ceph mode |
| [deploy/rook-ceph-lab/README.md](deploy/rook-ceph-lab/README.md) | Stand up Rook Ceph + KubeVirt/CDI |
| [docs/ROADMAP.md](docs/ROADMAP.md) | What's done, what's next (per the developer plan) |
| [CONTRIBUTING.md](CONTRIBUTING.md) | Conventions, how to add a driver/endpoint |

## Workspace layout

```
atlas/
├── crates/
│   ├── atlas-common/       # config, error type, tracing, id helpers
│   ├── atlas-api-types/    # shared serde DTOs (the wire/domain contract)
│   ├── atlas-driver-core/  # StorageDriver trait + DriverError + DriverRegistry
│   ├── atlas-driver-ceph/  # ceph/rbd CLI wrapper (real) + FakeCephDriver
│   ├── atlas-driver-k8s/   # kube-rs read-only StorageClass/PVC/PV listing
│   ├── atlas-inventory/    # SQLite read/upsert model + audit
│   ├── atlas-discovery/    # discovery worker (driver → inventory)
│   ├── atlas-gateway/      # axum server (bin: atlas-gateway) + tests
│   └── atlas-cli/          # atlasctl REST client
├── migrations/             # SQLite schema (0001_init.sql)
├── deploy/
│   ├── rook-ceph-lab/      # Rook Ceph + KubeVirt/CDI manifests + up.sh
│   └── k8s/                # atlas-gateway Deployment/RBAC/Service (fake + real ceph)
├── scripts/deploy-remote.sh
├── Dockerfile              # gateway image (fake/k8s modes)
└── Dockerfile.ceph         # gateway image + Ceph Reef client (real mode)
```

## Design authority

This implementation follows the developer plan
`Zyvor_Ceph_Integration_Developer_Implementation_Plan.pdf` (v1.0 engineering draft). Section
references (e.g. "PDF §10.2") throughout the code and docs point back to it.

## License

Proprietary — `LicenseRef-Zyvor-Proprietary`. Copyright (c) 2026 ZyvorAI Labs Private Limited.
