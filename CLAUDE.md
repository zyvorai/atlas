# Atlas — Zyvor Storage Control Plane

Atlas is the central storage control plane for the Zyvor product suite. Products (Zeus OS/v9s,
Veyron, Hyper2KVM, GuestKit, PacketWolf, Aether, Ragnarok, Machina, HyperSDK) call **Atlas APIs**;
Atlas talks to storage backends through **pluggable drivers**. Ceph is the first driver.

Design authority: `Zyvor_Ceph_Integration_Developer_Implementation_Plan.pdf` (v1.0 engineering draft).

## Current state (slices 1–5 done; verified on real Rook Ceph)
Implemented:
- `atlas-*` cargo workspace, SQLite (`sqlx`), axum 0.8 REST + `tonic` gRPC; async **job engine**.
- **Three backends** behind `StorageDriver`: real Ceph (`ceph`/`rbd` CLI) + **NFS** + **ZFS**,
  plus a fake Ceph driver and a live K8s driver. Discovery worker → SQLite inventory.
- Write path: volumes (PVC + direct RBD), snapshots/clone/restore, CephFS RWX, RGW buckets + backups
  (`export-diff`→S3, retention, presigned), scheduled snapshots/backups, per-tenant quotas + policies.
- Observability: monitor/alerts + webhook, `/metrics` (Prometheus self), `/metrics/{history,forecast,ceph}`,
  Ceph-native `/ceph/{status,osd-tree,osd-df,df}`, unified `/events`, `/readyz`; `deploy/observability/`.
- **React console** embedded in the gateway (HTTPS, login, Observatory, Ceph page).
  UI identity: **Soundings** bathymetric system — see `docs/ATLAS_UI_CONTRACT.md`
  and `crates/atlas-gateway/ui/src/atlas-soundings.css`.
- `deploy/rook-ceph-lab/` (single-node overlay + day-2 ops scripts) + `deploy/k8s/` ceph deployment.
- **DataBridge** (`atlas-databridge`): cloud-to-edge DB migration control plane. **Six source engines**
  — Postgres, MySQL, MariaDB (homogeneous → CNPG/Percona) + Oracle, SQL Server (heterogeneous → Postgres
  edge, seeded by Debezium's initial snapshot) + MongoDB (homogeneous document → Percona Server for
  MongoDB, Mongo Kafka sink) on Ceph. Real connectors: Postgres/MySQL/MariaDB default, SQL Server
  (`tiberius`)/Oracle (OCI)/MongoDB behind cargo features; precise CDC lag behind `kafka-lag`. Full
  pipeline (discover→assess→provision→full-load→CDC→validate→cutover→rollback); **Postgres verified
  end-to-end on two live Rook Ceph clusters incl. real Debezium CDC**; **all six source engines'
  real-connector discovery verified live** — MySQL 8.4, MariaDB 11.x, MongoDB 7.0 (replica set),
  SQL Server 2022, Postgres, and Oracle 23ai/26ai Free — on real servers through the deployed k3s gateway
  (binlog ROW / replica-set change streams / `is_cdc_enabled` / supplemental-log-min → `cdc_capable`,
  real db + table/collection names, PKs, counts). The gateway image now bundles the OCI Instant Client
  and builds the `oracle` feature. **Deeper stages: Postgres is verified through real CDC; MySQL, MariaDB,
  and MongoDB are verified through provision → real full-load (data physically copied source→edge) →
  validate (row/document-count parity)** against a lightweight edge. Streaming CDC + cutover for the
  non-Postgres engines still need the Kafka/Debezium stack (not installed on the shared lab).
  See `docs/DATABRIDGE.md` + `deploy/databridge/`.

Day-2 operations added (slices, all fake-first tested): control-plane durability (job recovery, graceful
shutdown, self-state backup, deep readyz/livez), alerting maturity (ack/silence + job/CDC/quota rules),
cluster-ops & maintenance (OSD ops, backend cordon, worker pause, job cancellation), governance
(token revocation, rate limiting, optional OIDC/SSO login alongside local username/password —
verified live against a throwaway Dex instance, `deploy/dex-lab/`), volume lifecycle (orphan GC,
QoS), DataBridge CDC self-heal,
upgrade pre-flight + rollback, and cross-cluster DR **scaffolding** (RBD-mirroring
peers/mirrors/failover API + jobs).

**Trial/licensing** (Ed25519-signed JWT, same design as `veyron::trial` + Aurora's
`gtm_api.middleware.license`): `crates/atlas-license` (verify + status, product tag
`atlas-trial`), `crates/atlas-license-tool` (sales-only `keygen`/`issue` CLI, never shipped to
customers), gateway wiring in `crates/atlas-gateway/src/license.rs` (`license_middleware` gates
the bearer-protected `api` router with 402 when expired; `GET /license/status` stays reachable
in `public_api` alongside `/auth/login` and `/auth/oidc/*`). `Config::license_enforce` defaults
to **true** (matching Aurora) — `make run`/`make run-databridge` explicitly set
`ATLAS_LICENSE_ENFORCE=false` so local fake-Ceph dev keeps working with zero setup, and
`deploy/k8s/atlas-gateway.yaml` currently does the same (no `atlas-license` Secret exists yet
for that manifest — flip to `true`, or remove the override, once a real customer token is
issued). See `docs/LICENSING.md`.

Deferred: **real** RBD-mirroring/DR verification (needs a 2nd cluster; the API/jobs are scaffolded but
the `rbd mirror` paths are unverified), per-product integrations beyond the gRPC surface.

## Layout
- `crates/atlas-common` — config, error, tracing, id helpers.
- `crates/atlas-api-types` — shared serde DTOs.
- `crates/atlas-driver-core` — `StorageDriver` trait + `DriverError` + `DriverRegistry`.
- `crates/atlas-driver-ceph` — `ceph`/`rbd` CLI wrappers + `FakeCephDriver`.
- `crates/atlas-driver-nfs` — `NfsDriver` (second backend; exports→pools, shares→filesystem volumes).
- `crates/atlas-driver-zfs` — `ZfsDriver` (third backend; zpools→pools, datasets→filesystem volumes).
- `crates/atlas-databridge` — DataBridge: source connectors, assessment, CNPG/Percona CR builders,
  pipeline stages, reconciler (cloud-to-edge DB migration; `migrations/0011`, `/api/atlas/v1/databridge/*`).
- `crates/atlas-driver-k8s` — `kube-rs` read-only StorageClass/PVC/PV listing.
- `crates/atlas-inventory` — SQLite read/upsert model.
- `crates/atlas-discovery` — discovery worker.
- `crates/atlas-gateway` — axum server (bin `atlas-gateway`).
- `crates/atlas-cli` — `atlasctl` REST client.
- `migrations/` — SQLite schema.
- `deploy/rook-ceph-lab/` — lab manifests + `up.sh` (`--single-node`; Rook v1.20.2 + Squid + CSI drivers).
- `scripts/deploy-remote.sh <host> <user>` — build (`Dockerfile`) + deploy the fake/k8s gateway to `zyvor-system`.
- `scripts/deploy-ceph-gateway-remote.sh <host> [user]` — build (`Dockerfile.ceph`, Squid client) + roll out
  the **real-Ceph** gateway `atlas-gateway-ceph` in `rook-ceph` (NodePort 30511). See `docs/DEPLOYMENT.md`.

## Conventions
- Every source file begins with `// Copyright (c) 2026 ZyvorAI Labs Private Limited. All rights reserved.`
- Match the monorepo Rust stack: axum 0.8, `sqlx` SQLite, `thiserror` 2.0 + `anyhow`, `tracing`.
- Ceph/rbd command wrappers use **arg-arrays only, never string concatenation**.

## Run locally (no Ceph, no cluster needed)
```
make run            # gateway with fake Ceph driver on 127.0.0.1:5110
cargo run -p atlas-cli -- --base-url http://127.0.0.1:5110 health
```
Live K8s path (needs KUBECONFIG): `atlasctl storage-classes`.

## Deploy script gotchas

- **`scripts/deploy-ceph-gateway-remote.sh` no longer restarts `atlas-gateway-ceph`
  unconditionally.** It used to run `kubectl rollout restart` after every `kubectl apply`,
  including no-op re-runs — on Ceph-RBD-backed storage (this Deployment's own PVC) that
  repeatedly interrupts an in-flight CSI mount and corrupts the volume (found live, 2026-08-31,
  chasing a stuck Atlas gateway on a second `../hypercluster`-provisioned host: `../hypercluster`'s
  own `storage apply` calls this script on every re-run, so simply re-running it — even just to
  validate an unrelated fix — kept bouncing a healthy pod and produced a multi-hour Ceph
  `HEALTH_ERR`/`recovery_unfound` cascade). Now it only restarts when `kubectl apply`'s output
  does **not** match `^deployment\.apps/${DEPLOY} unchanged$`. If you touch step 7/7 again, keep
  that gate — see `../zeus-os/CLAUDE.md`'s "unconditional restart on every re-apply" entry for
  the full writeup and two sibling-repo instances of the same bug (`../hypercluster`,
  `../packetwolf`).
