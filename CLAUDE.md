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

Deferred: RBD mirroring/DR (needs a 2nd cluster), per-product integrations beyond the gRPC surface.

## Layout
- `crates/atlas-common` — config, error, tracing, id helpers.
- `crates/atlas-api-types` — shared serde DTOs.
- `crates/atlas-driver-core` — `StorageDriver` trait + `DriverError` + `DriverRegistry`.
- `crates/atlas-driver-ceph` — `ceph`/`rbd` CLI wrappers + `FakeCephDriver`.
- `crates/atlas-driver-nfs` — `NfsDriver` (second backend; exports→pools, shares→filesystem volumes).
- `crates/atlas-driver-zfs` — `ZfsDriver` (third backend; zpools→pools, datasets→filesystem volumes).
- `crates/atlas-driver-k8s` — `kube-rs` read-only StorageClass/PVC/PV listing.
- `crates/atlas-inventory` — SQLite read/upsert model.
- `crates/atlas-discovery` — discovery worker.
- `crates/atlas-gateway` — axum server (bin `atlas-gateway`).
- `crates/atlas-cli` — `atlasctl` REST client.
- `migrations/` — SQLite schema.
- `deploy/rook-ceph-lab/` — lab manifests + `up.sh`.

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
