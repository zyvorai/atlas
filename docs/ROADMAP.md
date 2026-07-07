<!-- Copyright (c) 2026 ZyvorAI Labs Private Limited. All rights reserved. -->
# Roadmap

Atlas follows the phased plan in `Zyvor_Ceph_Integration_Developer_Implementation_Plan.pdf`
(§18 execution plan). This file tracks what's built vs. what's next.

## ✅ Slice 1 — MVP foundation (done, verified)

Read-only control plane + real Ceph lab.

- `atlas-*` Cargo workspace; axum 0.8 gateway; SQLite inventory.
- `StorageDriver` trait; real + fake Ceph drivers; live Kubernetes driver.
- Read-only REST API (backends, discover, clusters, nodes, osds, pools, volumes,
  storage-classes, metrics, alerts/jobs stubs).
- Discovery worker → normalized inventory; audit logging.
- Rook Ceph lab manifests (`deploy/rook-ceph-lab/`) + single-node overlay.
- k3s deployment (`scripts/deploy-remote.sh`, `deploy/k8s/`), fake + real (ceph) images.
- **Verified end-to-end** on real k3s + Rook Ceph: live k8s StorageClass/PV discovery, real RBD
  PVC provisioning, and Atlas real-driver discovery of the live cluster.

## ✅ Slice 2 — Write path & job engine (done, verified)

- **Async job engine** (`atlas-jobs`) — in-process tokio worker backed by `storage_jobs`, PDF §10.5
  state machine (`pending→queued→running→verifying→succeeded|failed`). SQLite-only (no Redis/NATS).
- `POST /volumes` → creates a Ceph-backed PVC; `202 Accepted` + job id.
- `DELETE /volumes/{id}`, `POST /volumes/{id}/expand`, `POST /volumes/{id}/snapshots`,
  `DELETE /snapshots/{id}` — all async jobs.
- Real `GET /jobs`, `GET /jobs/{id}`, `GET /snapshots`, `GET /policies`.
- `atlas-policy` — intent (`production|database|development|shared|ai`) → StorageClass + access/volume
  mode (PDF §12.3).
- `atlas-driver-k8s` write ops — create/get/delete/expand PVC + VolumeSnapshot (via DynamicObject).
- **Idempotency keys** on create (PDF §17.4); ownership recorded in `product_bindings`.
- `atlasctl create-volume|snapshot-volume|delete-volume|jobs|snapshots|policies`.
- **Verified end-to-end** on real k3s + Ceph: `POST /volumes` → PVC Bound on `zyvor-rbd-prod` →
  snapshot → VolumeSnapshot `readyToUse=true`, all driven through the job engine.

### Slice 2 follow-ups
- ✅ **Snapshot clone/restore** (`POST /snapshots/{id}/clone|restore`) — provision a new PVC from a
  VolumeSnapshot (`dataSource`), with parent/child dependency tracking
  (`storage_volumes.source_snapshot_id`) and a safe-delete guard (409 unless `?force=true`).
  Verified on real Ceph.
- ⏭ **Safe-by-default** deletes for production *volumes*: approval/confirmation (PDF §14, Rule 2).
  (Snapshot delete already guards on dependents; volume delete does not yet require approval.)
- ⏭ WebSocket/SSE job progress for the UI.
- ⏭ Direct RBD create path (bypassing CSI) for non-Kubernetes consumers.
- ✅ **Durable gateway DB** — the SQLite file is now backed by a `ReadWriteOnce` PVC (`Recreate`
  strategy). The real-mode gateway dogfoods `zyvor-rbd-prod` (Ceph); the fake-mode gateway uses the
  cluster default StorageClass so it still deploys without Ceph. Verified: inventory survives a pod
  restart. (For multi-replica HA, move to Postgres — the sqlx layer already abstracts this.)

## ✅ Slice 3 (part 1) — RGW object storage + backups (done, verified)

- `atlas-driver-rgw` — minimal S3 client for RGW (`rusty-s3` SigV4 signing + `reqwest`).
- Buckets via **ObjectBucketClaim**: `POST /buckets` (async job) creates an OBC; Rook provisions the
  bucket + Secret + ConfigMap; Atlas records the endpoint + a **secret reference** (never the keys).
  `GET /buckets`, `GET /buckets/{id}`.
- Backups: `POST /backup-jobs` snapshots the volume, writes a **backup manifest** (PDF §16.2) to the
  RGW bucket over S3, reads it back to **verify** (sha256 checksum). `GET /backups`, `GET /backups/{id}`.
- Credentials are read from the OBC Secret **in-cluster** by the job and never logged.
- migration 0003 (`storage_buckets`, `storage_backups`); single-node RGW object store + bucket SC.
- **Verified** on real Ceph RGW: bucket bound, manifest object written + verified, independently
  confirmed via `radosgw-admin bucket list`.

### RGW/backup follow-ups
- ✅ Full **data** backup (`POST /backup-jobs {"mode":"data"}`) — resolves the PVC's real RBD image
  (PV csi attributes), `rbd snap create` + `rbd export-diff` (capped 512 MiB), uploads the diff to
  RGW alongside the manifest. Verified on real Ceph (data object present, checksum recorded).
- ✅ **Restore-from-data** (`POST /restore-jobs {"mode":"data"}`) — creates an empty PVC, resolves its
  RBD image, downloads the diff from S3 (checksum-verified), and `rbd import-diff`s it into the new
  image. **Verified byte-for-byte on real Ceph**: wrote a file to volume A → data backup → restore to
  a new volume B → mounted B → the file read back identical.
  Follow-ups: multipart streaming for large images (current cap 512 MiB in memory).
- ✅ `POST /restore-jobs` — reads + checksum-verifies the backup manifest from RGW, then provisions
  a new PVC from the backup's VolumeSnapshot (PDF §16, DR-2). Verified on real Ceph.
- ✅ **Backup delete** (`DELETE /backups/{id}`) — removes the S3 manifest + `.rbd-diff` data objects
  (idempotent) and the RBD snapshot (best-effort), then the row. Verified live (RGW objects → 0, row 404).
- ✅ **Retention** — `ATLAS_BACKUP_KEEP` (0=unlimited) + per-request `keep`; on backup create, prunes
  backups beyond the keep count (most-recent-first) via delete jobs. Verified live (3 backups, keep=2 → 2).
- ✅ **Bucket delete** (`DELETE /buckets/{id}`) — deletes the OBC (Rook releases the bucket) + row;
  blocked with `409` if backups still reference the bucket, unless `?force=true`. Verified live
  (OBC NotFound, row 404).
- Bucket quotas + lifecycle policies; presigned download URLs; time-based backup retention.

## ✅ gRPC edge (done, verified)

- `tonic` 0.12 service `atlas.v1.AtlasStorage` served alongside REST on `ATLAS_GRPC_ADDR` (default
  `:5111`): `Health`, `ListClusters/Pools/Volumes`, `GetVolume`, `CreateVolume` (→ job), `GetJob`,
  `ListAlerts`. Server **reflection** enabled (grpcurl works without the proto).
- proto at `crates/atlas-gateway/proto/atlas.proto`; built with system `protoc` (installed in the
  Docker builders). Integration test drives the server with the generated client.
- **Verified** on the cluster with `grpcurl` (NodePort 30512): `ListPools` returned the real Ceph
  pools over gRPC via reflection.
- ✅ **JWT auth** — a tonic interceptor gated by `ATLAS_AUTH_REQUIRED` verifies an HS256 Bearer token
  in the `authorization` metadata with the shared secret (mirrors REST) and injects the actor.
  Verified live: no token → `Unauthenticated`, valid token → OK.
- ✅ **RBAC** — role hierarchy `viewer < operator < admin` (PDF §14.2; `product.service.*` = operator).
  Enforced on both edges (only when `auth_required`): writes need operator; delete-volume /
  create-backend / force-snapshot-delete need admin. Verified live (401/403/202/admin-gated).
- ✅ **WatchJob** server-streaming RPC — emits the job on each state change until terminal (verified
  live: `CreateVolume` → `WatchJob` streamed to `succeeded`).
- ✅ **REST SSE** parity: `GET /jobs/{id}/watch` streams `text/event-stream` job frames until terminal
  (verified live: running → succeeded).
- Follow-ups: gRPC clients in the products.

## ⏭ Slice 3+ — Enterprise & product integration
- Product integrations: Veyron (VM datastores), Hyper2KVM (direct-to-RBD migration), GuestKit
  (read-only snapshot inspection), PacketWolf (VM→OSD network/storage RCA), Ragnarok (AI storage
  facts), Aether (intent YAML), Machina (RGW/CephFS artifacts), HyperSDK (install/onboard CLI).
- ✅ **Monitor worker** (`atlas-monitor`): periodic discovery + alert rules (cluster unhealthy,
  pool near-full 75/85%, OSD down) → real `GET /alerts?state=`, `POST /alerts/evaluate`. Deterministic
  alert ids upsert on recurrence and resolve when conditions clear. Verified on real Ceph
  (`HEALTH_WARN` → one open cluster alert).
- ✅ **Ceph mgr Prometheus scrape**: each tick scrapes `rook-ceph-mgr:9283/metrics`, keeps a curated
  whitelist (capacity, OSD up/in/latency, pool usage, pg, health) into `storage_metrics`, and raises a
  high-OSD-latency alert (100ms warn / 1000ms critical). `GET /metrics/ceph?prefix=`. Verified on real
  Ceph (44 metrics incl. real capacity + `osd.0` apply latency). Follow-up: client IOPS/throughput
  metrics + recovery/backfill alerts.
- **Zeus OS UI** — Storage Center. Note two known collisions to resolve first:
  - `atlas` is already a Zeus OS module codename ("Machine Finder") in `v9s`.
  - A `ZeusStorageCenter.tsx` + `web/src/routes/storage.rs` already ship — decide whether Atlas's
    Storage Center *absorbs*, *replaces*, or *sits beside* them.
- Additional drivers behind the same trait: NFS, ZFS, SAN, cloud block, external Ceph import.
- Multi-tenancy: quotas, per-tenant policies, per-product service accounts (PDF §14).
- DR: RBD mirroring, secondary-cluster restore, one-click failover runbook (PDF §16.3).

## Known limitations (slice 1)

- Read-only: no provisioning/snapshot/backup yet (stubs return `NotImplemented`).
- Single Ceph backend id (`bkd_ceph_lab`) wired at startup; multi-backend registry exists but is
  not yet driven by `POST /backends` creating live drivers.
- `/nodes` derives from OSD hosts; `/alerts` and `/jobs` return `[]`.
- Pool `kind` is name-heuristic (doesn't yet read `ceph osd pool application` metadata).
- Single-node Ceph reports `HEALTH_WARN` (expected: 1 OSD < default size 3).
