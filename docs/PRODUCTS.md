<!-- Copyright (c) 2026 ZyvorAI Labs Private Limited. All rights reserved. -->
# Product integration (gRPC / ownership)

Atlas is the shared storage control plane. Products call **REST** and/or **gRPC**
(`atlas.v1.AtlasStorage` on `ATLAS_GRPC_ADDR`, lab NodePort **30512**).

## Owner convention

`CreateVolume` accepts an `Owner`:

| Field | Example | Meaning |
|---|---|---|
| `product` | `veyron`, `hyper2kvm`, `guestkit`, `packetwolf`, `aether`, `ragnarok`, `machina`, `hypersdk`, `zeus` | Product id recorded in `product_bindings` |
| `resource_type` | `vm`, `datastore`, `volume`, `fleet` | Product-side resource class |
| `resource_id` | product UUID / name | Product-side id |
| `role` | `owner` (default), `consumer` | Binding role |

Enumerate owned volumes with `ListVolumesByOwner(product, resource_id?)`.

## Auth

Same JWT hierarchy as REST: `viewer < operator < admin`. Product service accounts typically use
`product.service.<name>` (maps to operator) minted via `POST /api/atlas/v1/auth/token`.

## Surface today

Health, clusters/pools/volumes, create/delete/expand volume, snapshots, jobs (+ WatchJob stream),
alerts, **GetMetricsSummary**, **ListBuckets**.

Not yet on gRPC (REST-only): DR/mirroring, DataBridge, schedules, quotas admin, OSD ops, CephFS/NFS/ZFS
specifics.

## Follow-ups (product repos)

Generate/vendor stubs from `crates/atlas-gateway/proto/atlas.proto` into each product. Atlas does not
ship per-product workflow code — only the shared edge.
