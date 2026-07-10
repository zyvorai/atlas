<!-- Copyright (c) 2026 ZyvorAI Labs Private Limited. All rights reserved. -->
# Atlas API Reference (v1)

> Atlas also exposes a **gRPC edge** (`tonic`) on `ATLAS_GRPC_ADDR` (default `:5111`, NodePort 30512
> in the ceph deployment): service `atlas.v1.AtlasStorage` with `Health`, `ListClusters/Pools/Volumes`,
> `GetVolume`, `CreateVolume` (→ job; takes an `Owner` recorded in `product_bindings`), `DeleteVolume`
> (→ job, admin), `CreateSnapshot` (→ job, operator), `ListVolumesByOwner(product, resource_id?)` so a
> product enumerates only the volumes it owns, `GetJob`, `WatchJob` (server-streaming job updates),
> `ListAlerts`. Auth: HS256 JWT in `authorization` metadata when `ATLAS_AUTH_REQUIRED=1`, with the
> same role hierarchy as REST. Server reflection is enabled, so:
> `grpcurl -plaintext <host>:5111 list` and `grpcurl -plaintext <host>:5111 atlas.v1.AtlasStorage/ListPools`.
> The proto is at `crates/atlas-gateway/proto/atlas.proto`.

Base path: `/api/atlas/v1`. All responses are JSON. Errors use
`{ "error": { "code": "...", "message": "..." } }` with an appropriate HTTP status.

MVP slice 1 is **read-only** plus backend registration/discovery. Write endpoints
(`POST /volumes`, snapshots, clones) arrive in slice 2 — see [ROADMAP.md](ROADMAP.md).

Auth: when `ATLAS_AUTH_REQUIRED=1`, send `Authorization: Bearer <HS256 JWT>`.
When `0` (dev default), routes are open and the actor is `anonymous`.

## Meta

### `GET /health`
```json
{ "status": "ok" }
```

### `GET /version`
```json
{ "name": "atlas-gateway", "version": "0.1.0", "api": "v1" }
```

## Backends

### `GET /api/atlas/v1/backends`
List registered backends.
```json
[{ "id": "bkd_ceph_lab", "name": "zyvor-ceph-lab", "backend_type": "ceph",
   "mode": "managed_rook", "status": "active",
   "capabilities": { "block": true, "file": true, "object": true,
                     "snapshots": true, "clone": true, "expansion": true, "replication": true },
   "connection_ref": null }]
```

### `POST /api/atlas/v1/backends`
Register a backend row (no cluster lifecycle; starts `pending`).
```json
// request
{ "name": "ceph-prod", "backend_type": "ceph", "mode": "external" }
```
`backend_type` ∈ `ceph|nfs|zfs|san|cloud_block|kubernetes` (default `ceph`).
`mode` ∈ `managed_rook|external|read_only` (default `external`).

### `POST /api/atlas/v1/backends/{id}/discover`
Run a discovery pass for the backend and persist inventory (PDF §8.1). Writes an audit row.
```json
{ "state": "succeeded",
  "summary": { "backend_id": "bkd_ceph_lab", "cluster_id": "cls_5ace73d1-...",
               "pools": 1, "osds": 1, "volumes": 1 } }
```

## Clusters & inventory

### `GET /api/atlas/v1/clusters`
```json
[{ "id": "cls_5ace73d1-...", "backend_id": "bkd_ceph_lab", "name": "bkd_ceph_lab",
   "native_fsid": "5ace73d1-c30c-4e93-9970-d665de3b05a2", "health": "warn",
   "raw_capacity_bytes": 1000204886016, "used_capacity_bytes": 27631616,
   "available_capacity_bytes": 1000177254400 }]
```

### `GET /api/atlas/v1/clusters/{id}/health`
```json
{ "status": "warn", "summary": "WARN", "raw_capacity_bytes": 1000204886016,
  "used_capacity_bytes": 27631616, "available_capacity_bytes": 1000177254400,
  "recovering": false, "degraded_objects": 0 }
```

### `GET /api/atlas/v1/clusters/{id}/capabilities`
Returns the backend's capability flags for that cluster.

### `GET /api/atlas/v1/nodes`
Storage nodes (derived from distinct OSD hosts in the MVP): `[{ "host": "node01" }]`.

### `GET /api/atlas/v1/osds`
```json
[{ "id": 0, "cluster_id": "cls_5ace73d1-...", "up": true, "in_cluster": true,
   "device_class": "hdd", "host": null, "used_bytes": null, "capacity_bytes": null }]
```

### `GET /api/atlas/v1/pools`
Filters: `?backend=&kind=` (kind in rbd|cephfs_data|cephfs_metadata|rgw|nfs_export|other).
```json
[{ "id": "pool_1", "cluster_id": "cls_5ace73d1-...", "name": "rbd-nvme-prod",
   "kind": "rbd", "device_class": null, "replica_size": null,
   "used_bytes": 8192, "max_bytes": 950167011328, "health": "ok" }]
```
`kind` ∈ `rbd|cephfs_data|cephfs_metadata|rgw|other`.

### `GET /api/atlas/v1/volumes` · `GET /api/atlas/v1/volumes/{id}`
Filters: `?state=&tenant=&backend=&kind=` (kind in block|filesystem|object).
```json
[{ "id": "vol_rbd-nvme-prod_csi-vol-fe4aa484-...", "cluster_id": "cls_5ace73d1-...",
   "pool_id": "pool_1", "name": "csi-vol-fe4aa484-...", "kind": "block",
   "backend_native_id": "rbd-nvme-prod/csi-vol-fe4aa484-...",
   "size_bytes": 2147483648, "used_bytes": null, "state": "available",
   "health": "ok", "kubernetes_namespace": null, "pvc_name": null,
   "storage_class_name": null }]
```
`GET /volumes/{id}` returns `404 NOT_FOUND` when the id is unknown.

### `GET /api/atlas/v1/metrics/summary`
Aggregate capacity across clusters (PDF §13.2 overview cards).
```json
{ "raw_capacity_bytes": 1000204886016, "used_capacity_bytes": 27631616,
  "available_capacity_bytes": 1000177254400, "clusters": 1, "pools": 1, "volumes": 1 }
```

### `GET /api/atlas/v1/metrics/ceph[?prefix=ceph_osd]`
Latest Ceph metrics scraped from the mgr Prometheus module (PDF §15.1). A curated whitelist
(capacity, OSD up/in/latency, pool usage, pg, health), latest value per (name, labels).
```json
[{ "name": "ceph_cluster_total_bytes", "value": 1000204886016.0, "labels": {} },
 { "name": "ceph_osd_apply_latency_ms", "value": 11.0, "labels": { "ceph_daemon": "osd.0" } }]
```

### `GET /api/atlas/v1/alerts[?state=open]`
Alerts produced by the monitor worker (PDF §15.2): cluster unhealthy, pool near-full (75/85%),
OSD down, capacity forecast, **jobs failing (last 15m), CDC replication error, tenant quota
approaching (80/95%)**. Filter by `state` (`open`/`resolved`). Records also carry
`acknowledged_at`/`acknowledged_by`/`silenced_until`.
```json
[{ "id": "alert_cluster_unhealthy_cls_5ace73d1-...", "severity": "warning", "source": "monitor",
   "resource_type": "cluster", "resource_id": "cls_5ace73d1-...",
   "title": "Cluster health degraded", "description": "Cluster ... is HEALTH_WARN",
   "evidence": { "health": "warn" }, "state": "open", "created_at": "...", "resolved_at": null }]
```

### `POST /api/atlas/v1/alerts/evaluate`
Run the alert rules on demand (also runs every `ATLAS_MONITOR_INTERVAL_SECS`).
```json
{ "evaluated": true, "open_alerts": 1 }
```

### Alert lifecycle (operator, day-2)
- `POST /api/atlas/v1/alerts/{id}/ack` — record that an operator has seen it (not a resolve).
- `POST /api/atlas/v1/alerts/{id}/silence[?secs=3600]` — suppress webhook delivery for a window
  (default 1h, max 30d); the condition keeps being tracked and still shows in `/alerts`.
- `POST /api/atlas/v1/alerts/{id}/resolve` — operator override to resolve an open alert.

### Maintenance & cluster ops (admin, day-2)
- `POST /api/atlas/v1/backends/{id}/cordon` · `/uncordon` — stop / resume new provisioning onto a
  backend (existing volumes untouched). A create against a cordoned backend returns **503**.
- `GET /api/atlas/v1/maintenance` · `POST /api/atlas/v1/maintenance {"paused":true|false}` — pause /
  resume the job engine. Paused jobs stay `queued` (the worker holds them) and drain when resumed.
- `POST /api/atlas/v1/osds/{osd_id}/out` · `/in` · `/reweight?weight=0.8` — OSD maintenance as async
  jobs (`ceph osd out|in|reweight`); `202 + job id`.

## Write path (async jobs) — slice 2

All write operations enqueue a job and return **`202 Accepted`** with a `job_id`; poll
`GET /jobs/{id}` for progress. The job state machine is
`pending → queued → running → verifying → succeeded | failed` (PDF §10.5).

### `POST /api/atlas/v1/volumes`
Create a Ceph-backed volume (a PVC). Intent `policy` is resolved to a StorageClass by `atlas-policy`;
`kubernetes.storage_class` overrides it. Idempotent on `(tenant_id, name, size_bytes)` (PDF §17.4).
```json
// request
{ "tenant_id": "tenant_acme", "name": "billing-db-root", "size_bytes": 3221225472,
  "kind": "block", "policy": "database",
  "owner": { "product": "veyron", "resource_type": "virtual_machine",
             "resource_id": "vm_01", "role": "root_disk" },
  "kubernetes": { "namespace": "default", "create_pvc": true } }
// 202 response
{ "job_id": "job_3819884b6149", "state": "queued",
  "resource": { "volume_id": "vol_b16c40e12b76", "storage_class": "zyvor-rbd-prod",
                "namespace": "default", "pvc": "billing-db-root" },
  "links": { "job": "/api/atlas/v1/jobs/job_3819884b6149" } }
```

### `DELETE /api/atlas/v1/volumes/{id}`
Delete the PVC + inventory row (async job). `404` if the volume is unknown.

### `POST /api/atlas/v1/volumes/{id}/expand`
```json
{ "new_size_bytes": 6442450944 }
```
`400` if not larger than the current size.

### `POST /api/atlas/v1/volumes/{id}/snapshots`
Create a VolumeSnapshot from the volume's PVC (async job).
```json
{ "name": "optional-name", "snapshot_class": "zyvor-rbd-snapclass" }
```

### `POST /api/atlas/v1/snapshots/{id}/clone`
Provision a new **independent** volume (PVC) populated from the snapshot (`dataSource`). `name` is
required; `namespace`/`storage_class`/`size_bytes` default from the source volume.
```json
{ "name": "clone-of-demo", "namespace": "default" }
// 202 → resource: { volume_id, from_snapshot, mode: "clone", pvc, storage_class }
```

### `POST /api/atlas/v1/snapshots/{id}/restore`
Provision a point-in-time copy of the source volume from the snapshot. Same body as clone; `name`
defaults to `restore-<snap-suffix>`. `resource.mode` is `"restore"`.

### `DELETE /api/atlas/v1/snapshots/{id}[?force=true]`
Delete the VolumeSnapshot + snapshot row (async job). **Blocked with `409 CONFLICT`** if any volume
was cloned/restored from it (PDF §8.3); pass `?force=true` to override.

## Object storage & backups (RGW) — slice 3

### `POST /api/atlas/v1/buckets`
Provision an RGW bucket via an ObjectBucketClaim (async job).
```json
{ "name": "atlas-backups", "namespace": "rook-ceph", "storage_class": "zyvor-rgw-bucket",
  "max_objects": 1000, "max_size": "2G" }
// 202 → resource: { bucket_id, namespace }
```
`max_objects` / `max_size` (optional) set an RGW per-bucket quota (enforced via `radosgw-admin`).

### `GET /api/atlas/v1/buckets` · `GET /api/atlas/v1/buckets/{id}`
```json
[{ "id": "bkt_9624f5a6596f", "tenant_id": "global", "name": "atlas-backups",
   "bucket_name": "atlas-backups-63f4f511-...", "endpoint": "http://rook-ceph-rgw-...:80",
   "region": "us-east-1", "secret_ref": "atlas-backups", "namespace": "rook-ceph",
   "state": "bound", "created_at": "..." }]
```
`secret_ref` is the Kubernetes Secret name holding the S3 credentials — the keys are never returned.

### `DELETE /api/atlas/v1/buckets/{id}[?force=true]`
Delete the bucket: removes its ObjectBucketClaim (Rook releases the bucket) + row (async job).
**Blocked with `409`** if backups still reference the bucket; `?force=true` overrides. Requires operator.

### `POST /api/atlas/v1/backup-jobs`
Snapshot a volume and write a backup manifest to a (bound) bucket over S3, verifying the write.
```json
{ "volume_id": "vol_cfe1c97958f3", "bucket_id": "bkt_9624f5a6596f", "mode": "manifest",
  "keep": 0, "max_age_secs": 0 }
// 202 → resource: { backup_id, object_key, bucket_id }
```
`keep` (default `ATLAS_BACKUP_KEEP`, 0=unlimited) retains only the most recent N backups for the
volume, pruning older ones. `max_age_secs` (default `ATLAS_BACKUP_MAX_AGE_SECS`, 0=disabled) prunes
backups for the volume older than the cutoff. Both prune via delete jobs after the backup completes.
List with `GET /api/atlas/v1/backups?volume_id=<id>` to scope to one volume.
`mode` (default `manifest`) — `data` also exports the **real RBD image data** (`rbd export-diff`) to
`<object_key>.rbd-diff` in the bucket and records `data_bytes`/`data_checksum` in the manifest. The
data is **streamed** `rbd export-diff` → S3 multipart upload (16 MiB parts, sha256 over the stream) —
there is no in-memory size cap. Restore streams the object back into `rbd import-diff`.
`400` if the bucket is not `bound`; `404` if the volume/bucket is unknown.

### `GET /api/atlas/v1/jobs/{id}/watch`
**Server-Sent Events** (`text/event-stream`): emits the job on each state change until it reaches a
terminal state (REST parity with the gRPC `WatchJob` stream).
```
event: job
data: {"id":"job_...","state":"running","progress_percent":5,...}

event: job
data: {"id":"job_...","state":"succeeded","progress_percent":100,...}
```

### `POST /api/atlas/v1/restore-jobs`
Restore a volume from a backup (PDF §16, DR-2): the job reads + checksum-verifies the backup
manifest from RGW, then provisions a **new PVC from the backup's VolumeSnapshot**.
```json
{ "backup_id": "bkp_915f18cde6e7f0cf", "name": "restored-vol", "mode": "snapshot" }
// 202 → resource: { volume_id, from_backup, namespace, pvc }
```
`mode` (default `snapshot`) restores from the CSI VolumeSnapshot; `data` reconstructs the volume from
the RBD diff in S3 (creates an empty PVC → downloads + checksum-verifies the diff → `rbd import-diff`).
`404` if the backup is unknown.

### `DELETE /api/atlas/v1/backups/{id}`
Remove a backup: deletes its S3 manifest + `.rbd-diff` data objects (idempotent) and the RBD
snapshot (best-effort), then the row (async job). `202 + job id`. Requires operator.

### `GET /api/atlas/v1/backups/{id}/download?what=data|manifest`
Returns a **time-limited presigned S3 URL** for the backup object, signed with the bucket's
credentials (read in-cluster, never returned). The client downloads straight from RGW.
```json
{ "url": "http://rook-ceph-rgw-...svc/<bucket>/<key>?X-Amz-...", "object_key": "backups/.../....rbd-diff",
  "expires_in_secs": 900 }
```
`what` = `manifest` (default) or `data` (the `.rbd-diff` object). Needs a reachable cluster (`502` otherwise).
When `ATLAS_RGW_PUBLIC_ENDPOINT` is set the URL is signed against that public host (e.g.
`http://<node-ip>:30513/...`) so it resolves off-cluster; otherwise the bucket's in-cluster endpoint
is used. The signature binds to the host, so the client must connect to the endpoint in the URL.

### `GET /api/atlas/v1/backups` · `GET /api/atlas/v1/backups/{id}`
```json
[{ "id": "bkp_691e0b1e464b604c", "tenant_id": "global", "volume_id": "vol_cfe1c97958f3",
   "snapshot_id": "snap_697f4a9351c9", "bucket_id": "bkt_9624f5a6596f",
   "object_key": "backups/vol_cfe1c97958f3/bkp_691e0b1e464b604c.manifest.json",
   "format": "manifest-v1", "checksum": "8495f123...", "state": "verified", "created_at": "..." }]
```

## DataBridge — cloud-to-edge DB migration

Migrate managed cloud databases (PostgreSQL/MySQL) to edge databases on Ceph. Stage triggers return
`202 + job id` (track via `/jobs/{id}/watch`); reads return the inventory rows. Full reference +
runbook: [DATABRIDGE.md](DATABRIDGE.md).

### Sources
- `GET /api/atlas/v1/databridge/sources` · `POST /api/atlas/v1/databridge/sources` — list / register a source
  (`{name, kind: postgres|mysql, cloud, endpoint, port, database, secret_ref, secret_namespace, tls_mode, driver_mode: fake|real}`).
- `GET`/`DELETE /api/atlas/v1/databridge/sources/{id}` — get / delete.
- `POST /api/atlas/v1/databridge/sources/{id}/discover` — introspect the source schema (job).

### Plans & pipeline
- `GET /api/atlas/v1/databridge/plans` · `POST` — list / create (`{name, source_id, rollback_window_secs?}`).
- `GET /api/atlas/v1/databridge/plans/{id}` — plan detail (state, readiness_score, assessment).
- `POST /api/atlas/v1/databridge/plans/{id}/assess` — score readiness (job).
- `POST .../provision` — provision the edge DB (CloudNativePG/Percona) on Ceph (job).
- `POST .../full-load` — dump+load source → edge (batch job).
- `POST .../cdc/start` · `.../cdc/stop` — Debezium CDC control (job).
- `POST .../validate` — row-count/checksum compare (job).
- `POST .../cutover` — **admin, guarded** (validated + validation passed + CDC lag under threshold) (job).
- `POST .../rollback` — **admin**, within the rollback window (job).

### Read models
- `GET /api/atlas/v1/databridge/edge-clusters` · `/{id}`
- `GET /api/atlas/v1/databridge/cdc-streams` · `/{id}` (live lag)
- `GET /api/atlas/v1/databridge/validations[?plan_id=]`
- `GET /api/atlas/v1/databridge/cutovers`

## Jobs, snapshots, policies

### `GET /api/atlas/v1/jobs` · `GET /api/atlas/v1/jobs/{id}`
```json
{ "id": "job_3819884b6149", "tenant_id": "tenant_acme", "job_type": "volume.create",
  "state": "succeeded", "requested_by": "anonymous", "progress_percent": 100,
  "error": null, "result": { "bound": true, "phase": "Bound", "volume_id": "vol_b16c40e12b76" },
  "created_at": "...", "updated_at": "..." }
```

### `GET /api/atlas/v1/snapshots`
```json
[{ "id": "snap_29a557037a28", "tenant_id": "global", "volume_id": "vol_b16c40e12b76",
   "name": "billing-db-root-29a557037a28", "consistency": "crash", "state": "ready",
   "protected": false, "parent_snapshot_id": null, "created_at": "..." }]
```

### `GET /api/atlas/v1/policies`
The built-in intent → placement catalog (PDF §12.3).
```json
[{ "intent": "database", "storage_class": "zyvor-rbd-prod",
   "access_mode": "ReadWriteOnce", "volume_mode": "Filesystem",
   "description": "Databases — RBD NVMe, hourly snapshots, daily backup" }]
```

### `POST /api/atlas/v1/volumes/{id}/schedule` · `GET /schedules` · `DELETE /schedules/{id}`
Protection schedules: a background worker snapshots the volume every `interval_secs` and prunes its
scheduler-created snapshots to `keep`.
```json
// POST body (operator) — snapshot schedule
{ "interval_secs": 3600, "keep": 24 }
// POST body — backup schedule (to a bound bucket)
{ "kind": "backup", "bucket_id": "bkt_...", "interval_secs": 86400, "keep": 7, "mode": "data" }
// 201 → { "id": "sched_...", "volume_id": "vol_...", "kind": "snapshot", "interval_secs": 3600,
//         "keep": 24, "enabled": true, "next_run_at": "..." }
```
`kind` is `snapshot` (default) or `backup`; backups need a bound `bucket_id` (+ optional `mode`).
Worker cadence is `ATLAS_SNAPSHOT_TICK_SECS` (0 disables). Scheduled snapshots are named
`<volume>-sched-<id>`; retention only prunes scheduler-created snapshots/backups, never manual ones.

### `POST /api/atlas/v1/auth/tokens`
Mint a scoped service-account JWT for a product (admin). The shared secret never leaves Atlas.
```json
// body
{ "subject": "veyron", "role": "operator", "ttl_secs": 3600 }
// 201 → { "token": "<jwt>", "jti": "jti_...", "subject": "veyron", "role": "operator",
//         "level": 1, "expires_at": 1783480966, "ttl_secs": 3600 }
```
`role`: `viewer` (default), `operator`, `admin`, or `product.service.<name>` (→ operator). `ttl_secs`
is clamped to `[60, 7776000]` (90 days). The product sends the token as `Authorization: Bearer <jwt>`.
The response `jti` identifies the token for revocation.

### Token revocation & rate limiting (admin, day-2)
- `POST /api/atlas/v1/auth/tokens/{jti}/revoke` — kill a minted token before its TTL; the auth
  middleware then rejects it with **401** (a deny-list, checked per request).
- `GET /api/atlas/v1/auth/tokens/revoked` — the current revocation list.
- **Rate limiting**: set `ATLAS_RATE_LIMIT_RPM=N` (default `0` = off) to cap requests **per actor per
  minute** across `/api/atlas/v1/*`; over-limit requests get **429**.

### `GET /api/atlas/v1/tenants/{id}/quota` · `PUT .../quota`
Per-tenant storage quota + live usage (PDF §14 multi-tenancy). `PUT` (admin) sets the limits.
```json
// PUT body — 0 means unlimited for that dimension
{ "max_bytes": 1610612736, "max_volumes": 10 }
// GET/PUT reply
{ "tenant_id": "acme", "max_bytes": 1610612736, "max_volumes": 10,
  "used_bytes": 1073741824, "volume_count": 1 }
```
`POST /volumes` (and gRPC `CreateVolume`) rejects a create that would exceed either limit with `409`
(`resource_exhausted` on gRPC) *before* enqueueing the job. Usage is computed live from the tenant's
volume rows.

### `GET /api/atlas/v1/tenants/{id}/policies` · `PUT|DELETE .../policies/{intent}`
Per-tenant policy overrides: remap an intent to a specific placement for one tenant (admin to set).
```json
// PUT body
{ "storage_class": "zyvor-cephfs-shared", "access_mode": "ReadWriteMany", "volume_mode": "Filesystem" }
```
When a create names an intent (`policy`) and the tenant has an override, it wins over the built-in
catalog — precedence: request-pinned `kubernetes.storage_class` › tenant override › catalog.

## Live Kubernetes (served straight from the cluster)

### `GET /api/atlas/v1/storage-classes`
Ceph-backed classes are tagged `is_ceph: true` (PDF §7.1). `502 DRIVER_ERROR` if no cluster.
```json
[{ "name": "zyvor-rbd-prod", "provisioner": "rook-ceph.rbd.csi.ceph.com",
   "reclaim_policy": "Delete", "volume_binding_mode": "Immediate",
   "allow_volume_expansion": true, "is_ceph": true,
   "labels": { "zyvor.dev/storage-backend": "ceph", "zyvor.dev/storage-kind": "block",
               "zyvor.dev/live-migration": "review-required" } }]
```

### `GET /api/atlas/v1/kubernetes/pvcs` · `GET /api/atlas/v1/kubernetes/pvs`
Live PVC/PV listings (namespace/phase/storage class/capacity/csi driver).

## HTTP status codes

| Code | Meaning |
|---|---|
| 200 | OK |
| 400 | `VALIDATION_ERROR` |
| 401 | `AUTH_ERROR` (when auth required) |
| 404 | `NOT_FOUND` |
| 500 | `INTERNAL` / DB error |
| 502 | `DRIVER_ERROR` (backend/k8s unreachable) |

## `atlasctl` equivalents

```bash
atlasctl health                     # GET /health
atlasctl ready                      # GET /readyz (DB + driver + k8s deep-check)
atlasctl version                    # GET /version
atlasctl backends                   # GET /backends
atlasctl discover [backend]         # POST /backends/{backend}/discover  (default bkd_ceph_lab)
atlasctl clusters | pools | osds | volumes
atlasctl storage-classes            # GET /storage-classes (live k8s)
atlasctl metrics                    # GET /metrics/summary
atlasctl ceph-metrics [--prefix P]  # GET /metrics/ceph
atlasctl history [--minutes 60]     # GET /metrics/history (persisted time-series)
atlasctl forecast [--minutes 1440]  # GET /metrics/forecast (days-until-full projection)
atlasctl self-metrics               # GET /metrics (Prometheus text-exposition)
atlasctl ceph-status               # GET /ceph/status (live ceph status)
atlasctl ceph-osd-tree             # GET /ceph/osd-tree (CRUSH map)
atlasctl ceph-osd-df               # GET /ceph/osd-df (per-OSD utilization)
atlasctl ceph-df                   # GET /ceph/df (per-pool usage)
# write path (slice 2):
atlasctl policies                   # GET /policies
atlasctl create-volume NAME --size-gib 5 --policy database --namespace default
atlasctl snapshot-volume VOLUME_ID [--name NAME]
atlasctl clone-snapshot SNAPSHOT_ID --name NAME [--namespace NS]
atlasctl restore-snapshot SNAPSHOT_ID [--name NAME]
atlasctl delete-volume VOLUME_ID
atlasctl delete-snapshot SNAPSHOT_ID [--force]
atlasctl create-bucket NAME [--namespace rook-ceph]
atlasctl buckets
atlasctl backup-volume VOLUME_ID --bucket-id BUCKET_ID
atlasctl restore-backup BACKUP_ID [--name NAME]
atlasctl backups
atlasctl jobs [ID]                  # GET /jobs (or a single job)
atlasctl snapshots                  # GET /snapshots
# global flags: --base-url (ATLAS_BASE_URL), --token (ATLAS_TOKEN)
```
