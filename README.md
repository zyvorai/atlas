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

## Status — slices 1 & 2 done, **verified end-to-end on a real k3s + Rook Ceph cluster** ✅

**Slice 1 — read-only control plane:**
- `atlas-*` Cargo workspace; axum 0.8 gateway; SQLite (`sqlx`) inventory.
- Read-only REST discovery/inventory API (`/api/atlas/v1/...`).
- Pluggable `StorageDriver` trait: **real Ceph driver** (`ceph`/`rbd` CLI) + **fake driver**;
  **live Kubernetes driver** (StorageClasses / PVCs / PVs via `kube-rs`).
- `atlas-discovery` worker → normalized inventory; `deploy/rook-ceph-lab/` + `scripts/deploy-remote.sh`.

**Slice 2 — async write path:**
- **Job engine** (`atlas-jobs`): SQLite-backed tokio worker, PDF §10.5 state machine.
- `POST /volumes` (create Ceph-backed PVC), `DELETE`, `expand`, snapshots — all `202 + job id`.
- `atlas-policy` intent→placement; idempotency keys; ownership bindings.
- Real `GET /jobs`, `/snapshots`, `/policies`; snapshot **clone/restore** with a safe-delete guard.
- **Verified**: `POST /volumes` → PVC Bound on `zyvor-rbd-prod` → snapshot → clone/restore.
- Durable DB: SQLite backed by a Ceph PVC (survives pod restarts).

**Slice 3 — RGW object storage + backups:**
- `atlas-driver-rgw` S3 client; buckets via **ObjectBucketClaim** (`POST /buckets`); bucket quotas + stats.
- `POST /backup-jobs`: RBD `export-diff` streamed to RGW (multipart) + verify; restore-from-data;
  retention (keep-N + max-age); presigned downloads. **Verified** on real Ceph RGW.

**Slice 4 — edges, protection, multi-tenancy, observability:**
- **gRPC edge** (`tonic`) alongside REST: `WatchJob` streaming, product-integration surface, RBAC.
- **SSE** job progress; **monitor/alerts** worker (cluster/pool/OSD/capacity-forecast rules) + webhook notifier.
- CephFS RWX; direct RBD (provision/clone/resize/flatten/snap/rollback/du); scheduled snapshots & backups;
  per-tenant quotas + policy overrides; audit log; service-account JWTs.
- **Metrics**: `/metrics` (Prometheus self-metrics), `/metrics/history` (persisted time-series),
  `/metrics/forecast` (days-to-full), `/metrics/ceph`; `deploy/observability/` Prometheus + Grafana bundle.
- **Ceph-native introspection**: `/ceph/status`, `/ceph/osd-tree`, `/ceph/osd-df`, `/ceph/df`.

**Slice 5 — multi-backend + Zeus OS console:**
- **Three backends** behind `StorageDriver`: Ceph + **NFS** + **ZFS** (all in `/backends`, filters, gauges).
- `/backends/summary` + per-backend Prometheus gauges; `?backend=&kind=` filters on `/volumes` + `/pools`;
  `/volumes.csv` export; unified activity feed (`/events`); `/readyz` deep-check.
- **React/Vite console** (Zeus OS "Tahoe" design, embedded in the gateway): every capability wired,
  HTTPS, branded login, **Observatory** (6 live canvas visualizations), Ceph page, per-backend cards,
  and a **Nebula** default theme (+ Midnight, Aurora).
- **Day-2 ops**: `deploy/rook-ceph-lab/{reclaim-space,resize-osd,setup-k3s-disk,teardown}.sh`
  (Ceph capped to 400 GiB on `/dev/sdb1`; the freed tail becomes `/dev/sdb2` for the k3s data-dir).

**DataBridge — cloud-to-edge database mobility:**
- Migrate managed cloud databases (AWS RDS/Aurora, GCP Cloud SQL — PostgreSQL & MySQL) to open
  engines at the edge (**CloudNativePG / Percona on Ceph RBD**), from one control plane.
- Full pipeline — **discover → assess → provision → full-load → CDC (Debezium) → validate → cutover
  → rollback** — as async jobs with a reconciler; admin-guarded cutover, rollback window.
- **Fake-first**: the whole pipeline runs with no cloud/k8s (`make run-databridge`); real edge
  provisioning (operator CR apply + Ceph PVCs) is wired. Console: **DataBridge** section. See
  [docs/DATABRIDGE.md](docs/DATABRIDGE.md) + [deploy/databridge/](deploy/databridge/README.md).

See [docs/ROADMAP.md](docs/ROADMAP.md) and [docs/API.md](docs/API.md).

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

Then open the **Storage Center** — a Zeus OS-style React console — at **http://127.0.0.1:5110/** (or
`http://<node>:30511/` on the cluster). It's a React/Vite/Tailwind SPA (`crates/atlas-gateway/ui`)
embedded in the gateway binary, with live inventory, capacity/health, job progress (SSE), alerts,
metrics, tenants, and full write actions. Build it with `make ui` (or `make ui-dev` for hot reload).

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
