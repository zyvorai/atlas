# Atlas — Zyvor Storage Control Plane

Atlas is the central storage control plane for the Zyvor product suite. Products (Zeus OS/v9s,
Veyron, Hyper2KVM, GuestKit, PacketWolf, Aether, Ragnarok, Machina, HyperSDK) call **Atlas APIs**;
Atlas talks to storage backends through **pluggable drivers**. Ceph is the first driver.

Design authority: `Zyvor_Ceph_Integration_Developer_Implementation_Plan.pdf` (v1.0 engineering draft).

## Current state (MVP slice 1: read-only backend + Rook lab)
Implemented:
- `atlas-*` cargo workspace, SQLite persistence (`sqlx`), axum 0.8 gateway.
- Read-only REST discovery/inventory endpoints (`/api/atlas/v1/...`).
- `StorageDriver` trait with a **fake Ceph driver** (fixtures) and a **live K8s driver**
  (list StorageClasses / PVCs / PVs).
- `atlas-discovery` worker normalizing driver output into SQLite inventory.
- `deploy/rook-ceph-lab/` manifests to stand up Rook Ceph + KubeVirt/CDI.

Deferred (not built yet): write/provisioning path + job engine, gRPC edge, RGW/backup,
per-product integrations, Zeus OS UI.

## Layout
- `crates/atlas-common` — config, error, tracing, id helpers.
- `crates/atlas-api-types` — shared serde DTOs.
- `crates/atlas-driver-core` — `StorageDriver` trait + `DriverError` + `DriverRegistry`.
- `crates/atlas-driver-ceph` — `ceph`/`rbd` CLI wrappers + `FakeCephDriver`.
- `crates/atlas-driver-nfs` — `NfsDriver` (second backend; exports→pools, shares→filesystem volumes).
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
