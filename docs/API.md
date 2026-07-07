<!-- Copyright (c) 2026 ZyvorAI Labs Private Limited. All rights reserved. -->
# Atlas API Reference (v1)

> Atlas also exposes a **gRPC edge** (`tonic`) on `ATLAS_GRPC_ADDR` (default `:5111`, NodePort 30512
> in the ceph deployment): service `atlas.v1.AtlasStorage` with `Health`, `ListClusters/Pools/Volumes`,
> `GetVolume`, `CreateVolume` (→ job), `GetJob`, `WatchJob` (server-streaming job updates),
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
```json
[{ "id": "pool_1", "cluster_id": "cls_5ace73d1-...", "name": "rbd-nvme-prod",
   "kind": "rbd", "device_class": null, "replica_size": null,
   "used_bytes": 8192, "max_bytes": 950167011328, "health": "ok" }]
```
`kind` ∈ `rbd|cephfs_data|cephfs_metadata|rgw|other`.

### `GET /api/atlas/v1/volumes` · `GET /api/atlas/v1/volumes/{id}`
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
OSD down. Filter by `state` (`open`/`resolved`).
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
{ "name": "atlas-backups", "namespace": "rook-ceph", "storage_class": "zyvor-rgw-bucket" }
// 202 → resource: { bucket_id, namespace }
```

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
{ "volume_id": "vol_cfe1c97958f3", "bucket_id": "bkt_9624f5a6596f", "mode": "manifest", "keep": 0 }
// 202 → resource: { backup_id, object_key, bucket_id }
```
`keep` (default `ATLAS_BACKUP_KEEP`, 0=unlimited) retains only the most recent N backups for the
volume, pruning older ones.
`mode` (default `manifest`) — `data` also exports the **real RBD image data** (`rbd export-diff`) to
`<object_key>.rbd-diff` in the bucket and records `data_bytes`/`data_checksum` in the manifest.
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

### `GET /api/atlas/v1/backups` · `GET /api/atlas/v1/backups/{id}`
```json
[{ "id": "bkp_691e0b1e464b604c", "tenant_id": "global", "volume_id": "vol_cfe1c97958f3",
   "snapshot_id": "snap_697f4a9351c9", "bucket_id": "bkt_9624f5a6596f",
   "object_key": "backups/vol_cfe1c97958f3/bkp_691e0b1e464b604c.manifest.json",
   "format": "manifest-v1", "checksum": "8495f123...", "state": "verified", "created_at": "..." }]
```

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
atlasctl version                    # GET /version
atlasctl backends                   # GET /backends
atlasctl discover [backend]         # POST /backends/{backend}/discover  (default bkd_ceph_lab)
atlasctl clusters | pools | osds | volumes
atlasctl storage-classes            # GET /storage-classes (live k8s)
atlasctl metrics                    # GET /metrics/summary
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
