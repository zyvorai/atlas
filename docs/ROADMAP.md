<!-- Copyright (c) 2026 ZyvorAI Labs Private Limited. -->
<!-- SPDX-License-Identifier: AGPL-3.0-only OR LicenseRef-Atlas-Commercial -->
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
- ✅ **Safe-by-default** deletes for production *volumes*: `DELETE /volumes/{id}?confirm=true`
  required for prod/database StorageClasses (PDF §14 Rule 2; `force=true` accepted as alias).
  (Snapshot delete already guards on dependents; volume delete now requires confirmation.)
- ⏭ WebSocket/SSE job progress for the UI.
- ✅ **Direct RBD create path (bypassing CSI)** for non-Kubernetes consumers (machina/libvirt, bare
  VMs): `POST /rbd-images {name, size_bytes, pool?}` runs `rbd create` and records a volume row
  (native id `rbd:pool/image`, no PVC); `GET /rbd-images?pool=` lists from Ceph; `DELETE
  /rbd-images/{pool}/{image}` runs `rbd rm`. `atlasctl create-rbd-image`. Verified live: image
  created (`rbd info` 1 GiB), listed, and removed on real Ceph.
- ✅ **Direct RBD clone (golden image)**: `POST /rbd-images/{pool}/{image}/clone {name, snap?}`
  snapshots + protects the base and `rbd clone`s a COW copy — the machina/libvirt base-image →
  per-VM workflow. Verified live: clone's `rbd info` shows `parent: <pool>/golden-base@v1`.
- ✅ **Real usage stats**: `POST /rbd-usage/refresh` recomputes each volume's `used_bytes` via
  `rbd du` (resolving the image from the `rbd:` id or the PVC→PV for CSI volumes). Verified live:
  a fresh volume's image resolved and `used_bytes` written. `atlasctl refresh-usage`.
- ✅ **RBD resize + flatten**: `POST /rbd-images/{pool}/{image}/resize {size_bytes}` grows an image
  (updates inventory size); `.../flatten` detaches a COW clone from its parent snapshot. Verified
  live: base 1→3 GiB, clone flattened to independent. `atlasctl resize-rbd-image/flatten-rbd-image`.
- ✅ **Volume list filters**: `GET /volumes?state=&tenant=`. Verified live.
- ✅ **Raw-RBD snapshots**: `GET`/`POST /rbd-images/{pool}/{image}/snapshots`, `POST .../rollback`
  (point-in-time restore, admin). Verified live (create → list `[pit1]` → rollback).
- ✅ **Object-storage observability**: `GET /buckets/{id}/stats` (`radosgw-admin bucket stats` —
  object count, size, quota) and `GET /buckets/{id}/objects[?prefix=]` (S3 `ListObjectsV2`). Verified.
  `/stats` now has a Fake-mode branch (returns a zeroed-but-`available` stub) instead of crashing
  on deployments without a `radosgw-admin` binary; the underlying `radosgw-admin` call is bounded
  to 12s so a slow/unresponsive RGW soft-fails (`available: false`) rather than hanging.
- ✅ **Randomized-testing validation fixes** — found via a randomized battery of valid/boundary/
  invalid API probes: `POST /buckets` now rejects names under 3 characters (`400
  VALIDATION_ERROR`) matching the S3 bucket-naming rules the console already advertises, instead
  of silently accepting them (harmless in practice since Rook's OBC always generates a
  UUID-suffixed real bucket name regardless, but the gap let a caller's mistake through
  unflagged). `atlas_policy::resolve` now rejects an unrecognized `policy` intent with `400
  VALIDATION_ERROR` instead of silently falling back to the request kind's default placement — a
  typo in `policy` used to provision storage silently rather than failing loudly; a
  tenant-specific policy override (`PUT /tenants/{id}/policies/{intent}`, not restricted to the
  built-in catalog) still resolves correctly.

## ✅ Web dashboard — Storage Center (Zeus OS-style React console)

A full **React 19 + Vite + Tailwind + TanStack Query + zustand** SPA at `crates/atlas-gateway/ui`,
built to match the **Zeus OS (v9s)** look-and-feel — vendored HSL design tokens + `tahoe/zeus`
glass/pill/badge/table classes, the **mac-desktop shell** (glassy menu bar with cluster-health badge,
clock, spotlight ⌘K, live running-jobs indicator, and a Bearer-token control; grouped "Center"
sidebar; app dock), Inter type, Lucide icons, recharts. Built by a `node:22` Docker stage and
**embedded into the gateway binary** via `rust-embed`, served at **`/`** with an SPA fallback (open
`http://<node>:30511/`). Local dev: `make ui` (bundle) / `make ui-dev` (Vite proxy).

Every Atlas capability is wired: **Overview** (capacity gauge, health, client-I/O sparkline,
recovery, pools/OSDs, open alerts); **Volumes** (filters, create/expand/snapshot/schedule/delete +
detail drawer with labels & product bindings); **RBD Images** (create/clone/resize/flatten/delete,
snapshots list/create/rollback, usage refresh); **Snapshots**, **Schedules**, **Backups**
(create/restore/download/delete), **Buckets** (create/stats/object-browser/delete), **Alerts**
(state filter + evaluate), **Metrics** (Ceph, prefix filter), **Jobs** (live SSE progress),
**Audit** (filters), **Tenants** (quota + policy CRUD), **Access** (mint service-account JWTs),
**Policies**, **Backends**, **Kubernetes**, **Cluster**. Write actions thread `202 → SSE watch →
toast → refetch`. Replaces the earlier vanilla-JS `ui.html`. This standalone console sidesteps the
Zeus OS `atlas`/`ZeusStorageCenter.tsx` collisions; Zeus OS can later embed it or call the same API.

**UX polish**: a **branded login gate** (Zyvor hexagon mark, animated gradient orbs, optional
service-account token, live gateway health/version) with a session gate + sign-out; a **theme toggle**
(dark ↔ **Aurora** neon variant); confirm dialogs on destructive actions; loading/empty states with
CTAs; a running-jobs popover; ⌘K spotlight with arrow-nav; copy-to-clipboard IDs; a capacity radial +
per-pool utilization bars on the Command Deck; per-page titles, hexagon favicon, responsive sidebar.

- ✅ **Nav overflow-menu fix** — the "More sections"/"More shortcuts" fold-menus in the top nav (shown
  when the rail is too narrow to fit every section/shortcut) opened but rendered invisible: they used
  `position: absolute` inside `.at-rail-nav`/`.at-controls`, both `overflow: hidden`, so the panel was
  clipped before it painted. Fixed by portaling both to `document.body` with `position: fixed` +
  `getBoundingClientRect()`-computed coordinates, matching the pattern the section flyouts already used.
- ✅ **Two-shell Apple shop redesign** — **Carbon** (dark shop: black canvas + Apple Blue) and
  **Apple Lite** (light shop: `#F5F5F7` + Apple Blue). Elevated rounded boxes, SF system type,
  selection tiles, and swipe rails. Cosmic Orange reserved for the brand mark only. See
  `docs/ATLAS_UI_CONTRACT.md`. Preceded by an earlier Zeus two-shell port and sidebar chrome work;
  cascade-layer fixes for shell overrides remain in place.

**HTTPS**: an optional TLS listener (`ATLAS_HTTPS_ADDR` + `ATLAS_TLS_CERT`/`ATLAS_TLS_KEY` PEM), served
alongside HTTP via `axum-server` + rustls (ring provider — no cmake in the build). The ceph deployment
mounts a self-signed `atlas-tls` Secret and exposes NodePort **30543** (`https://<node>:30543/`; a real
cert/ingress gives trusted TLS).
- ✅ **`ATLAS_DISABLE_HTTP`**: found via the bank production-readiness audit — the plain HTTP
  listener always ran even when HTTPS was configured, so TLS could be bypassed entirely by
  hitting the HTTP port directly. This env var skips binding it; `validate_for_start()` refuses
  to boot with it set unless `ATLAS_HTTPS_ADDR`/`ATLAS_TLS_CERT`/`ATLAS_TLS_KEY` are all also
  configured (otherwise there'd be no REST listener at all). Left unset in the shipped manifests
  for now — flipping it on for a real deployment is a decision to make once HTTPS-only access is
  confirmed working end-to-end for every client, not an automatic default.
- ✅ **gRPC TLS**: the gRPC edge previously had no TLS option at all — plaintext regardless of how
  REST was configured. It now reuses the same cert/key as the REST HTTPS listener automatically
  (`tonic`'s `tls` feature, `Identity::from_pem` + `ServerTlsConfig`) whenever both are configured;
  stays plaintext otherwise (unchanged default). Verified live on the real-Ceph gateway (which
  already carries a real self-signed cert/key pair): `atlas-gateway gRPC (TLS) listening on
  0.0.0.0:5111` alongside REST HTTP/HTTPS, all three healthy. This same deploy pass also caught
  and fixed a real self-state-backup misconfiguration on that gateway (wrong RGW endpoint port —
  it was pointed at the presigned-URL-only service instead of the plain RGW NodePort, so uploads
  400'd); it now uploads successfully there too.
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
- ✅ **Multipart streaming** — data backup/restore no longer buffer the whole image. Backup streams
  `rbd export-diff` straight into an **S3 multipart upload** (16 MiB parts); restore streams the S3
  object into `rbd import-diff` stdin. sha256 is computed over the stream, so there is no in-memory
  size cap. Verified live: a 20 MiB random file → ~23 MiB diff (**2 parts**) → restore byte-identical
  (`data_verified: true`, source/restored sha256 match).
- ✅ `POST /restore-jobs` — reads + checksum-verifies the backup manifest from RGW, then provisions
  a new PVC from the backup's VolumeSnapshot (PDF §16, DR-2). Verified on real Ceph.
- ✅ **Backup delete** (`DELETE /backups/{id}`) — removes the S3 manifest + `.rbd-diff` data objects
  (idempotent) and the RBD snapshot (best-effort), then the row. Verified live (RGW objects → 0, row 404).
- ✅ **Retention (count)** — `ATLAS_BACKUP_KEEP` (0=unlimited) + per-request `keep`; on backup create,
  prunes backups beyond the keep count (most-recent-first) via delete jobs. Verified live (3 → keep=2 → 2).
- ✅ **Retention (age)** — `ATLAS_BACKUP_MAX_AGE_SECS` (0=disabled) + per-request/CLI `max_age_secs`; on
  backup create, prunes completed backups for the volume older than the cutoff via delete jobs.
  `GET /backups?volume_id=` filters by volume. Verified live (backup aged past a 3s cutoff → pruned).
- ✅ **Bucket delete** (`DELETE /buckets/{id}`) — deletes the OBC (Rook releases the bucket) + row;
  blocked with `409` if backups still reference the bucket, unless `?force=true`. Verified live
  (OBC NotFound, row 404).
- ✅ **Presigned download** (`GET /backups/{id}/download?what=data|manifest`) — returns a time-limited
  SigV4 URL signed with the bucket's in-cluster credentials (never exposed), so a client pulls the
  object straight from RGW. Verified live (credential-free download → 200, real `rbd diff v1` bytes).
- ✅ **Off-cluster downloads** — `ATLAS_RGW_PUBLIC_ENDPOINT` makes the gateway sign presigned URLs
  against a public RGW host (exposed via the `atlas-rgw-public` NodePort, 30513) so they resolve
  outside the cluster. Verified live (URL host = node:30513, credential-free GET → 200, `rbd diff v1`).
- ✅ **Bucket quotas** — `POST /buckets` accepts `max_objects`/`max_size`; the job sets the OBC
  `additionalConfig` and enforces the RGW per-bucket quota directly via `radosgw-admin`
  (`quota set`+`enable`). Verified live (`enabled: true, max_objects: 100, max_size: 1 GiB`).
- Bucket lifecycle policies; time-based backup retention; external RGW ingress for downloads.

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
- ✅ **Product-integration surface**: `CreateVolume` takes an `Owner {product, resource_type,
  resource_id, role}` recorded in `product_bindings`; `ListVolumesByOwner(product, resource_id?)`
  lets a product enumerate only the volumes it owns; `CreateSnapshot` (operator) and `DeleteVolume`
  (admin) return job replies. Verified live over gRPC reflection (Veyron flow: create-owned →
  list-by-owner scoped correctly → snapshot → delete, all jobs `succeeded`).
- ✅ **GetMetricsSummary** + **ListBuckets** — capacity/I/O/recovery rollup and RGW bucket inventory
  for product consoles (see `docs/PRODUCTS.md`).
- Follow-ups: generated gRPC client stubs vendored into each product (Veyron, Hyper2KVM, …).

## ✅ DataBridge — cloud-to-edge database mobility (six source engines)

A migration control plane layered on Atlas (`atlas-databridge`): migrate managed cloud databases
to open engines at the edge on Ceph-backed storage.

- Pipeline as async jobs + a reconciler: **discover → assess → provision → full-load → CDC → validate →
  cutover → rollback**. Admin-guarded cutover (validated + validation passed + CDC lag threshold); rollback window.
- **Six source engines**: Postgres, MySQL, MariaDB (homogeneous → CNPG/Percona) + Oracle, SQL Server
  (heterogeneous → Postgres edge, seeded by Debezium's initial snapshot) + MongoDB (homogeneous
  document → Percona Server for MongoDB, Mongo Kafka sink), all on Ceph-backed storage.
- Real connectors: Postgres/MySQL/MariaDB default; SQL Server (`tiberius`) / Oracle (OCI) / MongoDB
  behind cargo features; precise Kafka CDC lag behind the `kafka-lag` feature.
- CDC: **Debezium** on Strimzi/Kafka → JDBC sink to the edge DB (Postgres path).
- `migrations/0011_databridge.sql`; `/api/atlas/v1/databridge/*`; **DataBridge** console section;
  `deploy/databridge/` operator bundle + Connect image. Gateway image bundles the OCI Instant Client
  and builds the `oracle` feature.
- **Postgres verified end-to-end** on two independent live Rook Ceph clusters (`<ephemeral-ip>`,
  `<ephemeral-ip>`) including real CDC replication, driven through the deployed gateway.
- **All six engines' real-connector discovery verified live** — MySQL 8.4, MariaDB 11.x, MongoDB 7.0
  (replica set), SQL Server 2022, Postgres, Oracle 23ai/26ai Free — through the deployed k3s gateway
  (binlog ROW / replica-set change streams / `is_cdc_enabled` / supplemental-log-min → `cdc_capable`,
  real db + table/collection names, PKs, counts).
- **MySQL, MariaDB, MongoDB verified through provision → real full-load (data physically copied
  source→edge) → validate** (row/document-count parity) against a lightweight edge.
- **MariaDB + MongoDB CDC + cutover verified live** (2026-09-01) on the Rook Ceph lab with
  Strimzi/Debezium; MySQL CDC live for DATETIME (cutover still pending).
- Fake-first: the entire pipeline runs with no cloud/k8s (`make run-databridge`), CI-locked by
  `tests/databridge_pipeline.rs`.
- Follow-ups: MySQL cutover; Oracle / SQL Server heterogeneous CDC end-to-end; TLS for cloud SSL sources.

See [DATABRIDGE.md](DATABRIDGE.md).

## ✅ Day-2 operations (done, fake-first tested)

Atlas is operable, not just observe-and-provision. See [DAY2.md](DAY2.md) for the full operator
runbook; summary:

- **Control-plane durability**: job recovery on boot, graceful shutdown, deep `/readyz` (DB +
  driver + worker heartbeats), optional self-state SQLite→S3 backup.
- **Job cancellation**: `POST /jobs/{id}/cancel` (admin) — the single-worker queue's escape hatch
  for a job wedged inside a hung `ceph`/`rbd` call; `kill_on_drop` on every subprocess spawn
  actually kills the child, not just the Rust future. Verified live: cancelled a genuinely stuck
  `rbd snap unprotect`/`rbd.snap_delete` job mid-run on real Ceph. Also verified live: a
  `bucket.create` quota-set step stuck `running` indefinitely (unbounded `radosgw-admin` call)
  blocking the whole queue — cancel freed it immediately; every `radosgw-admin` call is now
  additionally bounded to 12s at the driver layer so this specific case self-resolves instead of
  needing manual intervention.
- **Alerting maturity**: ack/silence(`?secs=`)/resolve lifecycle; rules for jobs-failing, CDC
  replication error, and tenant-quota-approaching added to the original cluster/pool/OSD set.
  Verified live against a real open alert (ack → silence honored → resolve).
- **Cluster ops & maintenance**: backend cordon/uncordon, `POST/GET /maintenance {paused}` worker
  pause, OSD out/in/reweight, dynamic backend registration (`POST /backends
  {backend_type:"nfs"|"zfs"}` — instantiates a live driver + discovers immediately; verified live
  that an unreachable server still registers/discovers cleanly since NFS/ZFS are fixture-only).
- **Governance**: token revocation (`POST /auth/tokens/{jti}/revoke`), per-minute rate limiting,
  audit CSV export + retention pruning, chargeback.
- **Volume lifecycle**: orphan-backup GC (`GET /maintenance/orphans`), per-image QoS
  (iops/bps), safe resize-down (`allow_shrink` guard).
- **DataBridge self-heal**: `POST /databridge/plans/{id}/cdc/restart` + reconciler auto-restart
  (bounded, 3×) for a stalled CDC stream.
- **Upgrade pre-flight + rollback**: `GET /upgrade/preflight` gates `deploy-remote.sh`/
  `deploy-ceph-gateway-remote.sh`; `--rollback` reverts via `kubectl rollout undo`.
- **Cross-cluster DR scaffolding**: peers/mirrors catalog, enable/disable/promote/demote (role
  guards + `force`), preflight checklist, one-click failover runbook (`confirm`/`force`), RPO
  recording. Verified live (control-plane guards + preflight blockers + forced override) against
  the real lab; the underlying `rbd mirror` data-plane still needs a second Ceph cluster to be
  production-verified — see [DR.md](DR.md).

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
  Ceph (44 metrics incl. real capacity + `osd.0` apply latency).
- ✅ **Client I/O + recovery metrics/alerts**: the scrape also keeps per-pool client I/O counters
  (`ceph_pool_rd/wr[_bytes]` — rate them for IOPS/throughput) and recovery/backfill health
  (`ceph_pg_recovering/backfilling/*_wait`, `ceph_num_objects_degraded/misplaced/unfound`). A
  recovery alert warns while PGs recover/backfill and escalates to critical on unfound objects,
  clearing when done. Verified live: client counters populated; recovery alert stays clear on an idle
  cluster (no false positive).
- ✅ **Rook CRD read integration**: `atlas-driver-k8s::rook` reads Rook's own `ceph.rook.io/v1`
  CRs (`CephCluster`, `CephBlockPool`, `CephFilesystem`, `CephObjectStore`) as a second, precise
  source of truth alongside the existing `ceph`/`rbd` CLI path — additive, nothing CLI-based was
  removed. Pool `kind` classification (previously a pure name heuristic) is now overridden with an
  exact Rook-CR-derived kind during discovery when a k8s driver is attached
  (`atlas_driver_k8s::rook::known_rook_pool_kinds`, wired through `atlas-discovery::run_discovery`'s
  new `rook_pool_kinds` parameter, mirroring the existing `RbdOwners` enrichment). `GET
  /ceph/health-rollup` cross-checks the CLI-derived severity against `CephCluster.status.ceph.health`
  and surfaces a `rook_disagreement` when they differ, falling back to a coarser Rook-CRD-only
  rollup if the `ceph` CLI path itself fails but Rook is reachable. New additive `GET
  /ceph/rook-status` exposes the raw CephCluster/CephBlockPool/CephFilesystem/CephObjectStore CR
  view. `ATLAS_ROOK_NAMESPACE`/`ATLAS_ROOK_CLUSTER_NAME` (both default `rook-ceph`) configure it.
  **Verified live** against the real single-node Rook lab (`<ephemeral-ip>`, redeployed via
  `scripts/deploy-remote.sh`): `GET /ceph/rook-status` returned the real `CephCluster` (`phase:
  Ready, health: HEALTH_WARN`) plus the real `rbd-nvme-prod`/`zyvorfs`/`zyvor-rgw` CRs all
  `Ready`; `GET /ceph/health-rollup` showed `sources: ["cli","rook-crd"]` with no
  `rook_disagreement` (the CLI and Rook's own health agreed). This pass also caught and fixed a
  real RBAC gap: the `atlas-gateway` ServiceAccount's ClusterRole had no `ceph.rook.io` grant at
  all, so the very first live call 403'd (`cephclusters.ceph.rook.io is forbidden`) — the
  ClusterRole in `deploy/k8s/atlas-gateway.yaml` now grants `get/list/watch` on `cephclusters` and
  full CRUD on `cephblockpools`/`cephfilesystems`/`cephobjectstores`, plus `create`/`update`/
  `delete` on `storageclasses` (previously read-only) for the lifecycle-automation jobs below.
- ✅ **Rook lifecycle automation**: create/list/delete Ceph pools/filesystems/object stores through
  Rook CRs from the API instead of hand-edited YAML manifests, following the same async-job pattern
  `bucket.create` already uses (CR apply → poll `status.phase == "Ready"` → done). `POST`/`GET
  /ceph/pools` applies a `CephBlockPool` (replicated pools only — erasure coding isn't modeled) + a
  matching `zyvor.dev`-labelled StorageClass; `POST`/`GET /ceph/filesystems` applies a
  `CephFilesystem` + RWX CephFS StorageClass; `POST`/`GET /ceph/object-stores` applies a
  `CephObjectStore` + RGW bucket StorageClass. `DELETE /ceph/{pools,filesystems,object-stores}/
  {name}` removes the CR + StorageClass, guarded `409` while a volume/bucket still references it
  (`?force=true` to override) — `storage_buckets` gained a `storage_class` column (migration 0029)
  so the object-store guard can find dependent buckets, mirroring
  `storage_volumes.storage_class_name`'s existing role for pools/filesystems. `atlasctl
  {ceph-pools,create-ceph-pool,delete-ceph-pool}` (and the filesystem/object-store equivalents).
  CephCluster-level changes (OSD/mon topology) are explicitly out of scope — left as a documented
  follow-up, not built.
  **Verified live** against the real single-node Rook lab (`<ephemeral-ip>`): `POST /ceph/pools`
  created a real `CephBlockPool` (`replicated_size: 1, failure_domain: "osd"`, matching this
  cluster's actual 1-OSD capacity) that reached `phase: Ready`; a real PVC (`storageClassName:
  zyvor-atlas-test-pool-3`) bound against the StorageClass the job created
  (`kubectl get pvc` → `Bound`); `DELETE` removed the StorageClass and the PVC was cleaned up.
  Two real gaps found and fixed along the way: (1) the pool/filesystem/object-store `replicated`
  spec hard-coded `requireSafeReplicaSize: <size >= 3>`, an invented heuristic that doesn't match
  any static manifest in this repo — every one of `deploy/rook-ceph-lab/*.yaml` sets it `false`
  unconditionally (the lab never has enough failure domains for Ceph's "safe" check to pass);
  `crates/atlas-jobs/src/dispatch/rook.rs` now always passes `false`, like the manifests. (2) the
  gateway's ClusterRole had no RBAC for `ceph.rook.io` at all (see the read-integration entry
  above) — fixed in the same pass. **Known follow-up, not a bug in this code**: a real Ceph
  cluster defaults `mon_allow_pool_delete=false` (confirmed via Rook's own reconcile error,
  `Error EPERM: pool deletion is disabled`) — `DELETE /ceph/pools/{name}` correctly deletes the
  k8s-level CR + StorageClass, but Rook's *own* finalizer can't complete the underlying Ceph pool
  purge until an operator sets that mon config explicitly; until then the CR sits `Terminating`
  indefinitely (harmless — StorageClass is already gone, so nothing new can provision against it).
  A future pass could have the delete job set/restore that mon config around the delete the same
  way this verification did manually, or document it as an explicit day-2 prerequisite.
- ✅ **CephFS shared volumes (RWX)**: `POST /volumes {"policy":"shared"}` provisions a
  **ReadWriteMany** CephFS volume (`zyvor-cephfs-shared`) that multiple pods mount at once — for ISO
  libraries, templates, and multi-writer product data (GuestKit, Machina). Single-node CephFS
  filesystem + StorageClass manifest at `deploy/rook-ceph-lab/single-node/cephfs-sc.yaml`. Verified
  live on real Ceph: two pods mounted the same volume concurrently and saw each other's writes.
  The `shared` policy now also correctly tags the volume `kind: "filesystem"` in inventory
  (previously it silently defaulted to `kind: "block"` since the request's `kind` field is
  optional and policy resolution didn't correct it) — a mistagged CephFS volume looked exactly
  like a deleted RBD image to the real driver's RBD-only discovery and got silently pruned from
  inventory on the very next discovery pass, even though the PVC kept existing and consuming
  capacity, invisibly to Atlas. Verified live: created a `shared` volume, ran a discovery pass, it
  now survives (previously vanished from `GET /volumes` and returned `404`, requiring manual
  `kubectl delete pvc` to clean up the now-orphaned real resource).
- **Zeus OS UI** — Storage Center. Note two known collisions to resolve first:
  - `atlas` is already a Zeus OS module codename ("Machine Finder") in `v9s`.
  - A `ZeusStorageCenter.tsx` + `web/src/routes/storage.rs` already ship — decide whether Atlas's
    Storage Center *absorbs*, *replaces*, or *sits beside* them.
- Additional drivers behind the same trait: NFS, ZFS, SAN, cloud block, external Ceph import.
- ✅ **Scheduled snapshots (protection schedules)**: `POST /volumes/{id}/schedule {interval_secs, keep}`
  registers a schedule (`snapshot_schedules`); a background worker (`ATLAS_SNAPSHOT_TICK_SECS`) snapshots
  the volume on cadence and prunes its scheduler-created snapshots to `keep`. `GET /schedules`,
  `DELETE /schedules/{id}`, `atlasctl schedule-snapshots`. Verified live: snapshots appear on interval
  and retention holds steady at `keep` (delivers the policy catalog's "hourly snapshots").
- ✅ **Scheduled backups**: the same schedule with `kind:"backup"` (+ `bucket_id`, `mode`) runs
  periodic backups to RGW and prunes them to `keep` (delivers the "daily backup" half). Verified live:
  backups created on cadence, retention never exceeds `keep`. `atlasctl schedule-backups`.
- ✅ **Multi-tenancy quotas**: per-tenant `max_bytes` + `max_volumes` (`storage_tenant_quotas`,
  0 = unlimited). `PUT /tenants/{id}/quota` (admin) sets them; `GET` returns limits + live usage.
  `POST /volumes` (REST + gRPC `CreateVolume`) rejects an over-quota create (409 / `resource_exhausted`)
  before enqueueing. `atlasctl quota|set-quota`. Verified live: byte + count limits both enforced,
  raising the limit admits the create, usage tracked from live volume rows.
- ✅ **Service-account tokens**: `POST /auth/tokens` (admin) mints a scoped HS256 JWT
  (`sub`/`role`/`exp`) for a product/service account — the shared secret never leaves Atlas. TTL is
  clamped to `[60s, 90d]` and issuance is audited. `atlasctl issue-token`. Verified: minted operator
  token creates volumes, viewer is `403`, no token `401`, and a non-admin cannot mint (test); live
  the endpoint returns a valid JWT with the expected claims.
- ✅ **OIDC/SSO login**: `GET /auth/oidc/{status,login,callback}` (optional — auto-disabled unless
  `ATLAS_OIDC_ISSUER_URL`/`_CLIENT_ID`/`_REDIRECT_URL` are set) is a second way to *obtain* a
  console session — a successful OIDC round-trip mints the exact same kind of Atlas JWT
  `/auth/login` does, so it sits alongside local username/password login, not in place of it. PKCE
  + CSRF state + nonce; the `groups` ID-token claim maps to a role via
  `ATLAS_OIDC_ADMIN_GROUP`/`_OPERATOR_GROUP` (no match → `viewer`). Verified live end-to-end
  against a throwaway Dex instance (`deploy/dex-lab/`): three static test users in different Dex
  groups each landed with the correct Atlas role (admin/operator/viewer-by-default) after the full
  browser redirect → login → callback → session round-trip.
- ✅ **Tenant-scoped reads**: found via a production-readiness audit ahead of a prospective
  multi-tenant bank deployment — JWTs carried no `tenant_id`, so `GET /volumes`, `/volumes.csv`,
  `/volumes/{id}`, `/buckets`, and `/buckets/{id}` returned every tenant's data to any
  authenticated `viewer`, not just their own. `Claims`/`Actor` now carry `tenant_id` (set at mint
  time for local login, `POST /auth/tokens`, and OIDC login — the latter via a configurable
  `ATLAS_OIDC_TENANT_CLAIM` ID-token claim, defaulting to `"global"` when unset so existing
  single-tenant deployments are unaffected); `auth::tenant_scope`/`require_tenant` enforce it on
  those five routes, with `admin` staying cross-tenant by design. `crates/atlas-gateway/tests/
  tenant_isolation.rs` is the regression guard. Console users (`POST /auth/users`) and issued
  tokens can now be created with an explicit `tenant_id` (default `"global"`).
- ✅ **Secrets-manager integration (Vault + External Secrets Operator)**: `deploy/vault-lab/` —
  a throwaway Vault (dev mode) + ESO demonstrate the real pattern a bank deployment would use:
  secrets live in Vault, ESO syncs them into an ordinary K8s `Secret` via Vault's Kubernetes auth
  method, and Atlas's Deployment references that `Secret` through the exact same `secretKeyRef` it
  already uses for `atlas-gateway-auth` — no Atlas-side code or config shape changes needed.
  Verified live end-to-end against a dedicated demo secret (not the live `atlas-gateway-auth`,
  deliberately, to avoid disrupting the running gateways): Vault → K8s-Secret sync confirmed
  byte-for-byte identical, and a live rotation in Vault propagated to the K8s Secret automatically
  within the 30s refresh interval, with no `kubectl`/redeploy on the Atlas side. Adopting this for
  the real `atlas-gateway-auth` Secret is a two-field YAML change once a real Vault is available
  (see the README's "Adopting this for real").
- ✅ **JWT secret rotation**: previously a single static `ATLAS_JWT_SECRET` with no rotation path —
  changing it instantly invalidated every outstanding session/service-account token with no grace
  period. `ATLAS_JWT_SECRET_PREVIOUS` (optional) is still accepted for *validating* tokens (never
  for minting new ones) during a rotation window: set it to the outgoing secret when rotating to a
  new `ATLAS_JWT_SECRET`, deploy, then remove it once confident no outstanding token still uses it
  (bounded by each token's own TTL, capped at 90 days). `auth::decode_token` is the shared
  fallback-decode helper — both the REST `auth_middleware` and the gRPC interceptor use it, so the
  two edges can't drift on this behavior.
- ✅ **Audit-log export before pruning**: `ATLAS_AUDIT_EXPORT_URL` (optional, alongside the
  existing `ATLAS_AUDIT_RETENTION_DAYS`) batches rows due for retention pruning into one JSON POST
  to an external sink — SIEM webhook, Splunk HEC, Elastic/Fluent Bit HTTP input, anything that
  accepts JSON — and only deletes the rows that were actually exported
  (`atlas_monitor::audit_export::export_and_prune`); a failed export leaves rows in place for
  retry on the next 6h tick rather than silently dropping them, closing the "audit data just
  disappears with no external record" gap a compliance review would flag first. Straight-line
  `prune()` (no export) remains available when `ATLAS_AUDIT_EXPORT_URL` is unset.
- ✅ **SIEM-export network path verified against real infra**: `deploy/siem-lab/` stands up a
  minimal HTTP receiver in the lab (stdlib-only Python, no deps) standing in for a SIEM's
  ingestion endpoint (Splunk HEC, Elastic/Fluent Bit HTTP input, a generic webhook collector). A
  new opt-in live test (`crates/atlas-gateway/tests/audit_export.rs`'s
  `live_export_against_siem_lab_receiver`, `#[ignore]`d — run with `cargo test --test
  audit_export -- --ignored`) exercises the real `export_and_prune` HTTP client against it over
  the actual lab network, confirmed via the receiver's own logs receiving the exact expected
  payload — proof against an independently-implemented server, not just the existing tests'
  in-process axum mock agreeing with itself. Deliberately doesn't touch the live gateway's
  `ATLAS_AUDIT_EXPORT_URL`/`ATLAS_AUDIT_RETENTION_DAYS` — turning those on for real is a standing
  retention-policy change for the platform team to make deliberately, not a side effect of this.
- ✅ **Postgres HA scaffolding verified against real infra**: `deploy/postgres-lab/` stands up a
  throwaway Postgres in the lab; a new opt-in live test
  (`crates/atlas-inventory/tests/postgres_live.rs`'s `connect_and_migrate_against_real_postgres`,
  `#[ignore]`d, `--features postgres`) proves `connect_postgres()`/`migrate_postgres()` actually
  open a connection and apply `migrations-postgres/` against a real server — not just a compile
  check behind the feature flag. Also caught and fixed a real gap: `migrations-postgres/` had
  drifted 3 migrations behind `migrations/` (missing `rbd_snapshots`, the native-id unique index,
  and — critically — this session's own `console_users.tenant_id` tenant-isolation column), now
  back in file-for-file parity and re-verified idempotent (migrate-twice is a no-op, matching what
  a rolling pod restart does). This proves the connection/schema half of Phase-1 HA; the
  SQLite-only query layer (`SqlitePool` used throughout `atlas-inventory`) is unchanged and still
  the explicitly out-of-scope larger effort — see `deploy/postgres-lab/README.md` and the
  known-limitations note below.
- ✅ **Supply-chain audit gate**: `cargo deny check` (CI job + `make audit`, `deny.toml`) — known-
  vulnerable/yanked advisories, disallowed licenses, unknown registries/git sources. Scoped to
  default features (what's actually shipped); the optional DataBridge connectors are compile-
  checked separately and excluded here since `tiberius` (SQL Server) currently pulls in an
  unmaintained/vulnerable rustls 0.21-era chain not worth blocking the default build over. Found
  and fixed a real memory-exhaustion DoS (RUSTSEC-2026-0195) by bumping `rusty-s3` 0.5→0.10 (zero
  code changes needed — it also dropped the vulnerable `quick-xml` dependency entirely). The
  handful of remaining "unmaintained, no safe upgrade available" advisories (`rsa`'s Marvin Attack
  timing side-channel; a small family of unmaintained-but-not-vulnerable crates transitive via
  `kube-runtime` 0.95) are explicitly `ignore`d with justification, re-checked whenever deps update.
- ✅ **Console password hashing**: switched from plain SHA-256 to Argon2id (`crates/atlas-gateway/
  src/auth.rs`'s `hash_password`/`verify_password_hash`) — a leaked `console_users` table can no
  longer be brute-forced offline at GPU/ASIC speed. Backward-compatible: existing `sha256$...`
  hashes still verify, so no forced password reset on upgrade; every newly hashed/changed password
  gets the new Argon2id format.
- ✅ **No plaintext secrets in committed manifests**: `ATLAS_ADMIN_PASSWORD` and
  `ATLAS_OIDC_CLIENT_SECRET` moved from a literal `value:` in `deploy/k8s/atlas-gateway*.yaml` to
  `secretKeyRef` (`scripts/ensure-atlas-auth-secret.sh` now also generates a strong
  `admin-password`; the OIDC secret points at the same `dex-oidc-client` Secret `deploy/dex-lab/
  up.sh` already creates, rather than duplicating the value). `Config::validate_for_start()` now
  also refuses to boot with `ATLAS_AUTH_REQUIRED=1` and a weak/default admin password, mirroring
  the existing JWT-secret guard.
- ✅ **Audit-log query API**: `GET /audit` (operator) returns the compliance trail newest-first with
  `actor`/`action`/`resource_type`/`resource_id` filters + bounded `limit`. Verified live.
- ✅ **Enriched `/metrics/summary`**: capacity-used %, snapshot/bucket/backup counts, client-I/O
  rollups (`ceph_pool_rd/wr[_bytes]`) and recovery status. Verified live.
- ✅ **gRPC parity**: `ExpandVolume` + `ListSnapshots` added to the edge (verified via grpcurl).
- ✅ **Volume ownership + labels**: `GET /volumes/{id}/bindings` lists product ownership;
  `GET`/`PUT /volumes/{id}/labels` reads/merges user labels into volume metadata. Verified live.
- ✅ **`GET /tenants`**: overview of every tenant with volumes/quota (usage + limits). Verified live.
- ✅ **Per-tenant policies**: `PUT /tenants/{id}/policies/{intent}` (admin) remaps an intent to a
  specific StorageClass + access/volume mode for one tenant; `create_volume` applies it (precedence:
  request-pinned SC › tenant override › built-in catalog). `GET`/`DELETE` too; `atlasctl
  set-tenant-policy`. Verified live: `acme/database` → CephFS RWX while `globex/database` stayed on
  RBD RWO, and deleting the override reverted `acme` to the catalog.
- Multi-tenancy: **complete** (quotas + service accounts + per-tenant policies, PDF §14).
- DR: RBD mirroring, secondary-cluster restore, one-click failover runbook (PDF §16.3).

## Known limitations (current)

- **DR data-plane unverified**: RBD-mirroring catalog, failover API, and jobs are complete and
  fake-mode-tested, but the real `rbd mirror` CLI paths need a live second Ceph cluster to be
  production-verified (`dataplane_verified` is hard-coded `false` until that drill runs). See
  [DR.md](DR.md).
- **arm64 container images: not shipped, tested and rejected for now.** A real timing probe
  (`.github/workflows/ci.yml`'s `docker` job, temporarily, on a throwaway branch/PR) built the
  full `Dockerfile` (heaviest image — all DataBridge features: librdkafka/cmake, ODPI-C, tiberius)
  for `linux/arm64` via QEMU emulation on the standard `ubuntu-latest` runner: it was cancelled
  after 70+ minutes still running, vs. ~10.5 minutes for a native amd64 build of the lighter
  `Dockerfile.ceph` in the same run — 6.7x+ and still climbing, not a one-off fluke. QEMU
  full-emulation is the wrong approach for this workspace's dependency weight; a real fix would
  need native cross-compilation (a proper `aarch64-unknown-linux-gnu` toolchain + cross-linked C
  deps) rather than `platforms: linux/amd64,linux/arm64` on the existing `docker/build-push-action`
  step, which is a materially larger effort than the one-line change this looked like at first.
- **Bank-grade hardening gaps** (found via a production-readiness audit; tenant-scoped reads,
  plaintext-secret-in-manifest, weak password hashing, missing supply-chain scanning,
  destructive-audit-pruning-with-no-export, and rate-limiting/self-state-backup-off-by-default
  from that audit are already fixed above — both `deploy/k8s/atlas-gateway*.yaml` now ship
  `ATLAS_RATE_LIMIT_RPM=600` and a working `ATLAS_STATE_BACKUP_*` block against a dedicated RGW
  user/bucket): the gateway is single-replica with a `Recreate` rollout (planned downtime per
  deploy — SQLite's query layer has no Postgres port yet; the connection/migration scaffolding
  behind the disabled `postgres` feature is now verified against a real Postgres
  (`deploy/postgres-lab/`), but that only proves connect+migrate work, not that Atlas can run its
  reads/writes against Postgres — porting the query layer is still a separate, larger, deferred
  effort); OIDC/SSO is only verified against a throwaway Dex instance, not a real enterprise IdP.
  The audit-log SIEM export and secrets-manager integration *patterns* are now both verified
  against real (lab) infra (`deploy/siem-lab/`, `deploy/vault-lab/` — see below); a real deployment
  still needs the bank's actual SIEM/Vault swapped in for the lab ones. None of these block a
  non-production pilot; all are gates before a production go-live.
- **Non-Postgres DataBridge streaming CDC + cutover**: MySQL is now verified end-to-end
  including real streaming CDC (provision → full-load → CDC → validate, against a real
  Kafka/Strimzi/Debezium stack) — but only for `DATETIME` columns; Debezium encodes MySQL
  `TIMESTAMP` columns as ISO-8601 strings the JDBC sink can't bind, an open gap needing a
  follow-up SMT fix (see `docs/DATABRIDGE.md`). MariaDB and MongoDB are verified through
  provision → real full-load → validate (row/document-count parity) only; their streaming-CDC
  and cutover stages haven't been run against the Kafka/Debezium stack yet. SQL Server and
  Oracle are verified through discovery only. Cutover itself (for any engine except Postgres)
  hasn't been driven live yet.
- **NFS/ZFS drivers are fixture-only**: `POST /backends {backend_type: "nfs"|"zfs", server, ...}`
  does instantiate a live driver instance and run it through discovery immediately (verified
  live) — but `atlas-driver-nfs`/`atlas-driver-zfs` are explicitly MVP/architecture-proof drivers
  (see their crate-level doc comments) that never actually connect to the given server: they
  always report the same deterministic capacity fixture (8 TB / 30% used) regardless of the
  address, rather than running `showmount`/`df` or `zpool list`/`zfs list`. Only the Ceph driver
  talks to real infrastructure today.
- Pool `kind` is name-heuristic when no k8s driver/Rook CRs are available (non-Rook Ceph). On a
  Rook-managed cluster it's now overridden with an exact classification read from live
  `CephBlockPool`/`CephFilesystem`/`CephObjectStore` CRs (see the Rook CRD read integration entry
  above) — implemented and unit-tested, pending a live-cluster verification pass.
- Single-node Ceph reports `HEALTH_WARN` (expected: 1 OSD < default size 3).
- Per-product integrations beyond the gRPC surface (Veyron VM datastores, Hyper2KVM direct-to-RBD
  migration, GuestKit, etc.) are not yet built — see the per-product rows above.
- `scripts/test-connectors.sh` (container-backed DataBridge connector tests) needs podman/docker;
  the `kafka-lag` feature needs `cmake` to build `rdkafka` — both are opt-in and skipped where that
  tooling isn't present.
