<!-- Copyright (c) 2026 ZyvorAI Labs Private Limited. All rights reserved. -->
# Atlas API Reference (v1)

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

### `GET /api/atlas/v1/alerts`
Returns `[]` (alert engine lands in a later slice).

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

### `DELETE /api/atlas/v1/snapshots/{id}`
Delete the VolumeSnapshot + snapshot row (async job).

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
atlasctl delete-volume VOLUME_ID
atlasctl jobs [ID]                  # GET /jobs (or a single job)
atlasctl snapshots                  # GET /snapshots
# global flags: --base-url (ATLAS_BASE_URL), --token (ATLAS_TOKEN)
```
