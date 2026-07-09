<!-- Copyright (c) 2026 ZyvorAI Labs Private Limited. All rights reserved. -->
# Atlas Architecture

Atlas is a storage **control plane**: a stable API in front of pluggable storage backends.
Its job is to let every Zyvor product consume storage by *intent* while Atlas owns the messy
parts — backend drivers, discovery, normalized inventory, ownership mapping, and audit.

## Why a control plane (and not per-product Ceph clients)

Before Atlas, storage logic was scattered: `hyper2kvm` shipped its own ceph-csi StorageClasses,
`machina` shelled out to `rbd`, `v9s` drove raw PVCs, and everyone else passed a bare
`storage_class` string. That duplicates logic and couples every product to Ceph.

Atlas centralizes it:

- **One API** — products never talk to Ceph directly.
- **Pluggable drivers** — Ceph today; NFS/ZFS/SAN/cloud later, with no product changes.
- **Backend-agnostic model** — a `StorageVolume` may be backed by RBD now and SAN tomorrow.
- **Ownership + audit** — every volume maps to product/resource/tenant/policy/pool/cluster.

## Crate topology

```
atlas-api-types  ─────────────┐ (DTOs: the shared contract)
        ▲                      │
        │                      ▼
atlas-common            atlas-driver-core ──► StorageDriver trait, DriverError, DriverRegistry
(config/error/ids)              ▲   ▲
        ▲                       │   │
        │            ┌──────────┘   └──────────┐
        │      atlas-driver-ceph          atlas-driver-k8s
        │      (real + fake)              (live StorageClass/PVC/PV)
        │            ▲
        │      atlas-inventory (SQLite RO model + audit)
        │            ▲
        │      atlas-discovery (driver → inventory)
        │            ▲
        └───── atlas-gateway (axum server: state, auth, routes)  ◄── atlas-cli (atlasctl)
```

Dependency rule: the gateway and discovery worker depend only on the **`StorageDriver` trait**,
never on a concrete backend. Adding a backend = implementing the trait + registering it.

**DataBridge** (`atlas-databridge`) layers a cloud-to-edge DB migration control plane on the same
foundation — it reuses the job engine, inventory (`migrations/0011_databridge.sql`), gateway, and
console, adds source connectors + operator/Debezium CR builders, and a periodic **reconciler** (mirrors
`spawn_scheduler`) that advances long-running pipeline work (edge CR readiness, full-load/validation
Jobs, CDC health). It calls Atlas to provision Ceph-backed storage for the edge databases. See
[DATABRIDGE.md](DATABRIDGE.md).

## The driver contract

`atlas-driver-core::StorageDriver` (PDF §17.2):

```rust
#[async_trait]
pub trait StorageDriver: Send + Sync {
    fn backend_id(&self) -> &str;
    async fn discover(&self) -> Result<DiscoveryResult, DriverError>;
    async fn health(&self)   -> Result<StorageHealth, DriverError>;
    async fn list_pools(&self)               -> Result<Vec<StoragePool>, DriverError>;
    async fn list_volumes(&self, pool: &str) -> Result<Vec<StorageVolume>, DriverError>;
    async fn metrics(&self)  -> Result<Vec<MetricSample>, DriverError>;
    // write path (MVP slice 2) — default to Err(DriverError::NotImplemented)
    async fn create_volume / expand_volume / delete_volume / create_snapshot / clone_snapshot / delete_snapshot
}
```

Read methods are live in slice 1. Write methods default to `NotImplemented` so slice 2 is
purely additive.

### Drivers today

| Driver | Crate | What it does | Notes |
|---|---|---|---|
| Real Ceph | `atlas-driver-ceph` | Parses `ceph status`, `ceph df detail`, `ceph osd tree`, `rbd ls -l` into DTOs | Arg-arrays only, `--format json`. `ATLAS_CEPH_DRIVER_MODE=real`. |
| Fake Ceph | `atlas-driver-ceph` | Deterministic fixtures | For local dev/tests/demo. `ATLAS_CEPH_DRIVER_MODE=fake`. |
| Kubernetes | `atlas-driver-k8s` | Lists StorageClasses / PVCs / PVs via `kube-rs`; tags Ceph-backed classes | Always live when a cluster is reachable. |

## Request flow (discovery)

```
POST /api/atlas/v1/backends/{id}/discover
        │
        ▼
gateway: resolve driver from DriverRegistry ──► atlas-discovery::run_discovery
        │                                              │
        │                                              ▼
        │                                    driver.discover()  (ceph/rbd CLI or fixtures)
        │                                              │
        │                                              ▼
        │                             atlas-inventory::upsert_discovery (SQLite, one tx)
        │                                   cluster → pools → osds → volumes
        ▼
audit row (storage_audit_logs) + JSON summary back to caller
```

Reads (`/pools`, `/volumes`, `/clusters`, …) come straight from the SQLite inventory.
`/storage-classes`, `/kubernetes/pvcs`, `/kubernetes/pvs` are served **live** from the k8s driver.

## Data model (SQLite)

`migrations/0001_init.sql` translates the plan's PostgreSQL schema (PDF §11) to SQLite
(`TIMESTAMPTZ`→RFC3339 `TEXT`, `JSONB`→`TEXT` + `json_valid()` CHECK, `BIGSERIAL`→`INTEGER PK
AUTOINCREMENT`). Core tables:

| Table | Purpose |
|---|---|
| `storage_backends` | Registered backends (type, mode, capabilities, secret **reference** only) |
| `storage_clusters` | Discovered clusters (fsid, capacity, health) |
| `storage_pools` | Normalized pools (kind rbd/cephfs/rgw, device class, replica size) |
| `storage_osds` | OSD up/in state, device class, host |
| `storage_volumes` | Zyvor volume abstraction (FK → cluster/pool) |
| `product_bindings` | Ownership: which product/resource/role owns a volume/bucket/share |
| `storage_snapshots` | Snapshot metadata + parent/clone dependency (slice 2) |
| `storage_jobs` | Async job records (slice 2) |
| `storage_policies` | Intent → placement/protection/backup/security (slice 2) |
| `storage_alerts` | Alert records (slice 2) |
| `storage_audit_logs` | Every sensitive action (PDF §14.3) |

Ownership mapping (`product_bindings`) is what lets Atlas answer "which VM owns this RBD image?"
and "which OSDs affect this VM?" — required for RCA, billing, and safe cleanup.

## Security posture

- **Secret references, never secrets** — backends store a reference (Kubernetes Secret / Vault
  path); no keyrings/keys in Atlas tables (PDF §14.1).
- **Real Ceph credentials stay in-cluster** — the real-mode gateway renders `/etc/ceph` from the
  Rook mon Secret via an initContainer; the admin key never leaves the cluster.
- **JWT auth** — `ATLAS_AUTH_REQUIRED=1` requires an HS256 Bearer token; dev default is open.
- **Audit everything** — discovery, backend registration, and (in slice 2) all write actions.

## Configuration

All via environment (see `.env.example`), loaded by `atlas-common::Config::from_env()` with
secret-redacting `Debug`:

| Env | Default | Meaning |
|---|---|---|
| `ATLAS_BIND_ADDR` | `127.0.0.1:5110` | Gateway listen address |
| `ATLAS_DATABASE_URL` | `sqlite://atlas.db?mode=rwc` | SQLite URL (WAL + FK on) |
| `ATLAS_CEPH_DRIVER_MODE` | `fake` | `real` (ceph/rbd CLI) or `fake` (fixtures) |
| `ATLAS_KUBECONFIG` | *(unset)* | Explicit kubeconfig; else in-cluster/default |
| `ATLAS_JWT_SECRET` | dev default | HS256 secret (set ≥32 bytes in prod) |
| `ATLAS_AUTH_REQUIRED` | `0` | Require JWT on `/api` routes |
| `RUST_LOG` | `info` | tracing filter |
