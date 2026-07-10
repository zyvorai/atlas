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
- **Zeus OS React console** embedded in the gateway (HTTPS, login, Observatory, Ceph page, Nebula theme).
- `deploy/rook-ceph-lab/` (single-node overlay + day-2 ops scripts) + `deploy/k8s/` ceph deployment.
- **DataBridge** (`atlas-databridge`): cloud-to-edge DB migration control plane. **Six source engines**
  — Postgres, MySQL, MariaDB (homogeneous → CNPG/Percona) + Oracle, SQL Server (heterogeneous → Postgres
  edge, seeded by Debezium's initial snapshot) + MongoDB (homogeneous document → Percona Server for
  MongoDB, Mongo Kafka sink) on Ceph. Real connectors: Postgres/MySQL/MariaDB default, SQL Server
  (`tiberius`)/Oracle (OCI)/MongoDB behind cargo features; precise CDC lag behind `kafka-lag`. Full
  pipeline (discover→assess→provision→full-load→CDC→validate→cutover→rollback); **Postgres verified
  end-to-end on two live Rook Ceph clusters incl. real Debezium CDC**; **MySQL (8.4) + MariaDB (11.x) +
  MongoDB (7.0, replica set) + SQL Server (2022) real-connector discovery verified live** on real servers
  (binlog ROW / replica-set change streams / `is_cdc_enabled` → `cdc_capable`, real db + table/collection
  names & counts) through the deployed k3s gateway; only Oracle remains fake-first (needs the `oracle`
  build feature + OCI Instant Client baked into the image). See `docs/DATABRIDGE.md` + `deploy/databridge/`.

Day-2 operations added (slices, all fake-first tested): control-plane durability (job recovery, graceful
shutdown, self-state backup, deep readyz/livez), alerting maturity (ack/silence + job/CDC/quota rules),
cluster-ops & maintenance (OSD ops, backend cordon, worker pause), governance (token revocation, rate
limiting), volume lifecycle (orphan GC, QoS), DataBridge CDC self-heal, upgrade pre-flight + rollback,
and cross-cluster DR **scaffolding** (RBD-mirroring peers/mirrors/failover API + jobs).

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
- `deploy/rook-ceph-lab/` — lab manifests + `up.sh`.
- `scripts/deploy-remote.sh <host> <user>` — build (`Dockerfile`) + deploy the fake/k8s gateway to `zyvor-system`.
- `scripts/deploy-ceph-gateway-remote.sh <host> [user]` — build (`Dockerfile.ceph`, bundles the Ceph client) + roll out the **real-Ceph** gateway `atlas-gateway-ceph` in `rook-ceph` (NodePort 30511, `imagePullPolicy: Never`).

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
