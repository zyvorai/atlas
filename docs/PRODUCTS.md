<!-- Copyright (c) 2026 ZyvorAI Labs Private Limited. -->
<!-- SPDX-License-Identifier: LicenseRef-Zyvor-Production-1.0 -->
# Product integration (gRPC / ownership)

Atlas is the shared storage control plane. Products call **REST** and/or **gRPC**
(`atlas.v1.AtlasStorage` on `ATLAS_GRPC_ADDR`, lab NodePort **30512**).

## Owner convention

`CreateVolume` accepts an `Owner`:

| Field | Example | Meaning |
|---|---|---|
| `product` | `veyron`, `hyper2kvm`, `guestkit`, `packetwolf`, `aether`, `ragnarok`, `machina`, `hypersdk`, `zeus`, `relay`, `kryton` | Product id recorded in `product_bindings` |
| `resource_type` | `vm`, `datastore`, `volume`, `fleet`, `database` | Product-side resource class |
| `resource_id` | product UUID / name | Product-side id |
| `role` | `owner` (default), `consumer`, `data_disk` | Binding role |

The product named **Transiva** is recorded as owner id `hyper2kvm`. That wire id is unchanged.

Enumerate owned volumes with `ListVolumesByOwner(product, resource_id?)`.

## Auth

Same JWT hierarchy as REST: `viewer < operator < admin`. Product service accounts typically use
`product.service.<name>` (maps to operator) minted via `POST /api/atlas/v1/auth/token`.

## Surface today

| RPC | Notes |
|---|---|
| `Health` | Liveness + version |
| `ListClusters` / `ListPools` / `ListVolumes` / `GetVolume` | Inventory reads |
| `CreateVolume` | Operator; returns `job_id` + `volume_id`; optional `Owner` |
| `DeleteVolume` | Admin; enqueues delete job |
| `ExpandVolume` | Operator; enqueues expand job (`new_size_bytes` must grow) |
| `CreateSnapshot` / `ListSnapshots` | Operator create; list optional `volume_id` filter |
| `ListVolumesByOwner` | Product-scoped volume enumeration |
| `GetJob` / `WatchJob` | Job status + server stream until terminal |
| `ListJobs` | Optional `state` filter; `limit` default 50 |
| `ListTenants` | Tenants with volumes or quotas (`id`/`name` ← `tenant_id`) |
| `ListAlerts` | Optional `state` filter |
| `GetMetricsSummary` | Capacity + client I/O + recovery rollup |
| `ListBuckets` | RGW object-bucket inventory |

Not yet on gRPC (REST-only): DR/mirroring, DataBridge, schedules, quotas admin, OSD ops, CephFS/NFS/ZFS
specifics.

## Relay (reliability control plane)

Zyvor Relay keeps the **event/job ledger in Postgres**. Atlas owns the **data disk PVC** (and
optional RGW backups), not the application rows.

| Field | Relay convention |
|---|---|
| `product` | `relay` |
| `resource_type` | `database` |
| `resource_id` | `postgres-primary` |
| `role` | `data_disk` |
| Intent | `policy: "database"` → RBD PVC (e.g. claim `postgres-data` in `zyvor-relay`) |

Integration lives in the Relay repo: `docs/ATLAS_STORAGE.md`,
`scripts/atlas-provision-relay-storage.sh`, `k8s/postgres-atlas.example.yaml`.

Service account: mint JWT subject `product.service.relay` (operator).

## Kryton (Windows virtualization)

[Kryton](../../tt/kryton) runs Windows guests on KubeVirt. It discovers StorageClasses
(optionally via Atlas) and stamps `disk.storageClass` on VM DataVolumes.

| Field | Kryton convention |
|---|---|
| `product` | `kryton` |
| `resource_type` | `vm` |
| `resource_id` | Kryton machine UUID |
| `role` | `data_disk` (boot / data PVC) |

Configure from Kryton **Settings → Integrations → Atlas** (`docs/ATLAS.md` in the Kryton repo).
Service account: mint JWT subject `product.service.kryton` (operator).

Probe from Kryton: `POST /api/v1/integrations/atlas/test` against Atlas `/readyz` +
`/api/atlas/v1/storage-classes`.

## Follow-ups (product repos)

Generate/vendor stubs from `crates/atlas-gateway/proto/atlas.proto` into each product. Atlas does not
ship per-product workflow code — only the shared edge.
