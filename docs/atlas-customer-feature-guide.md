# Atlas — Feature Guide

> **The central storage control plane for the Zyvor product suite.**

Atlas lets products ask for intent — "give me production block storage" — instead of wrestling with pool internals, then maps that intent to a real backend through pluggable drivers. It gives you block, file, and object storage from a single gateway, with async provisioning, snapshots, backups, replication, per-tenant governance, and a live console on top. Ceph is the first driver (RBD, CephFS, RGW/S3), with NFS and ZFS backends alongside it and DataBridge adding cloud-to-edge database and object mobility.

**3** Storage backends (Ceph · NFS · ZFS) · **6** Database engines migratable via DataBridge · **80+** REST endpoints across the control plane · **3** Access surfaces — REST · gRPC · SSE

This is the customer-facing feature reference. A print-ready PDF of the same content sits alongside this file. Generated from the product's actual capabilities.

## Contents

1. [Control Plane & Architecture](#1-control-plane-architecture)
2. [Block, File & Object Storage](#2-block,-file-object-storage)
3. [Data Protection](#3-data-protection)
4. [Multi-Backend & Ceph Operations](#4-multi-backend-ceph-operations)
5. [Observability & Metrics](#5-observability-metrics)
6. [Alerting & Day-2 Ops](#6-alerting-day-2-ops)
7. [Governance & Multi-Tenancy](#7-governance-multi-tenancy)
8. [DataBridge — Database Mobility](#8-databridge-—-database-mobility)
9. [DataBridge — Object Migration](#9-databridge-—-object-migration)
10. [Disaster Recovery](#10-disaster-recovery)
11. [Storage Center Console](#11-storage-center-console)

## 1. Control Plane & Architecture

_An intent-driven gateway that decouples every product from the storage underneath it._

- **Intent-Based Provisioning** — Products request an intent class ("production block storage") and Atlas resolves it to a concrete backend, pool, and placement. — _Callers stay decoupled from Ceph or any future backend — no pool internals leak into product code._
- **Pluggable Storage Drivers** — A single StorageDriver trait fronts every backend, with real Ceph, NFS, ZFS, and Kubernetes drivers plus a fake driver for local runs. — _Add a backend once and every product gets it through the same stable API._
- **Atlas Gateway** — An axum server that centralizes auth, audit, and the API surface, and embeds the Storage Center console in the binary. — _One deployable front door for storage across all nine Zyvor products._
- **REST + gRPC Surfaces** — The same control plane is reachable over REST and a tonic gRPC edge with streaming WatchJob for product integrations. — _Human tools use REST; product runtimes get typed, streaming gRPC._
- **Inventory & Discovery** — A discovery worker normalizes backend state into a SQLite inventory of pools, volumes, and ownership bindings. — _A consistent, queryable picture of every asset regardless of which backend holds it._
- **Durable, Self-Healing State** — Restarts recover the job engine (interrupted jobs fail safe, queued jobs re-enqueue), with graceful SIGTERM drain and deep readiness probes. — _The control plane stays trustworthy across pod restarts and rollouts._

> Core principle: products request intent, not pool internals. Atlas maps intent to a backend and owns inventory, ownership, and audit.

## 2. Block, File & Object Storage

_Provision and manage RBD block, CephFS file, and RGW/S3 object storage from one API._

- **Block Volumes** — Create, get, expand, and delete Ceph-backed block volumes (PVCs) as async jobs that return a job id immediately. — _Fast, non-blocking volume lifecycle with a durable audit of every change._
- **CephFS File Shares (RWX)** — Provision shared read-write-many file storage on CephFS for workloads that need concurrent access. — _Multi-writer file storage without standing up a separate NAS._
- **Object Buckets** — Create S3 buckets via ObjectBucketClaim with per-bucket quotas, stats, and presigned upload/download URLs. — _Self-service object storage with quota control and short-lived access links._
- **Direct RBD Image Ops** — Provision, clone, resize, flatten, snapshot, and roll back RBD images directly, with per-image usage tracking. — _Full low-level control when you need to bypass the PVC abstraction._
- **Safe Resize (Grow & Shrink)** — Expand volumes freely; shrink is opt-in behind an explicit allow_shrink flag as a data-loss guard. — _Reclaim over-provisioned space without accidentally destroying data._
- **Kubernetes Storage Inventory** — Read-only listing of StorageClasses, PVCs, and PVs live from the cluster via kube-rs. — _See exactly how Atlas storage surfaces inside Kubernetes._

## 3. Data Protection

_Snapshots, clones, and off-cluster backups — scheduled, verified, and retention-managed._

- **Snapshots** — Take point-in-time volume snapshots, then clone or restore from them behind a safe-delete guard. — _Instant rollback points without copying whole volumes._
- **Clone & Restore** — Spin a new volume from a snapshot or restore a volume in place from any snapshot. — _Branch environments from a known-good state in seconds._
- **Off-Cluster Backups** — Stream RBD export-diff to RGW/S3 as verified multipart uploads, with restore-from-data and presigned downloads. — _Durable, verifiable backups that survive loss of the source pool._
- **Scheduled Snapshots & Backups** — Register recurring snapshot and backup schedules per volume, driven by background workers. — _Set-and-forget protection instead of manual, forgettable runs._
- **Retention Policies** — Keep-N plus max-age retention automatically prunes old backups (and object versions). — _Bounded storage cost without hand-pruning stale copies._
- **Orphan Backup GC** — Surface backups whose source volume is gone so they can be cleaned up deliberately. — _No silent buildup of orphaned data you're still paying to store._

## 4. Multi-Backend & Ceph Operations

_Run Ceph, NFS, and ZFS side by side — and operate Ceph natively when it's the driver._

- **Three Storage Backends** — Ceph, NFS, and ZFS all live behind the same StorageDriver, with filters, per-backend gauges, and a backends summary. — _Mix backends under one control plane and one API._
- **Dynamic Backend Registration** — Register a live NFS or ZFS backend over the API and Atlas instantiates the driver and discovers it immediately. — _Onboard new storage without redeploying the gateway._
- **Backend Cordon & Drain** — Cordon a backend to reject new provisioning (503) while you drain and service it, then uncordon. — _Take storage offline for maintenance without breaking callers._
- **Ceph-Native Introspection** — Read ceph status, osd-tree, osd-df, and df directly through dedicated endpoints. — _Diagnose the real cluster without leaving the control plane._
- **OSD Management** — Mark OSDs in or out and reweight them via ceph osd commands from the API. — _Rebalance and service OSDs through the same audited surface._
- **QoS & Pool Migration** — Set per-image IOPS/BPS QoS limits and migrate images between pools with rbd migration. — _Tune performance and rebalance placement without downtime._

> OSD, QoS, and pool-migration operations act on a real Ceph cluster — they are exercised fake-first in CI and fully verified against live Rook Ceph.

## 5. Observability & Metrics

_Live capacity, health, forecasts, and a cinematic console view of the whole estate._

- **Prometheus Metrics** — Self-metrics at /metrics plus Ceph metrics and per-backend gauges for scraping. — _Drop Atlas straight into your existing monitoring stack._
- **Persisted Time-Series History** — Capacity and usage history is persisted and served at /metrics/history for trend analysis. — _See where storage has been, not just where it is now._
- **Capacity Forecast** — A days-to-full forecast projects when pools and backends will run out of headroom. — _Buy or reclaim capacity before you hit the wall._
- **Observatory** — Six live canvas visualizations render the cluster, pools, jobs, and flows in real time in the console. — _An at-a-glance operations view that reads like mission control._
- **Unified Activity Feed** — A single /events stream merges activity across every backend and subsystem. — _One timeline for everything happening in storage._
- **Grafana Bundle** — A ready-to-apply Prometheus + Grafana observability bundle ships in deploy/observability. — _Stand up dashboards in minutes instead of building them from scratch._

## 6. Alerting & Day-2 Ops

_Atlas is operable, not just observe-and-provision — with alerts, maintenance, and safe upgrades._

- **Rule-Based Alerting** — Rules cover cluster health, pool near-full, OSD down/latency, capacity forecast, recovery, failing jobs, CDC errors, and tenant-quota thresholds. — _Catch storage problems by policy, not by luck._
- **Alert Lifecycle** — Acknowledge, silence for a window, or resolve alerts, with a single webhook sink for downstream notification. — _Route real signal to your team and mute the noise._
- **Maintenance Pause** — Pause the job worker so new work holds in queue until you resume — cluster-wide freeze for maintenance. — _Do risky work with nothing new landing mid-flight._
- **Upgrade Pre-Flight** — A preflight check blocks upgrades on HEALTH_ERR clusters, open critical alerts, in-flight jobs, or lagging CDC. — _Never ship an upgrade into an unhealthy cluster._
- **Gated Rollout & Rollback** — deploy-remote.sh auto-gates on pre-flight and supports rollout undo, with a --force override. — _Ship and un-ship with guardrails baked into the script._
- **Control-Plane State Backup** — Optional VACUUM INTO snapshots of the control-plane DB stream to S3/RGW on a schedule with keep-N retention. — _Recover the control plane itself, not just the data it manages._

## 7. Governance & Multi-Tenancy

_Tenant isolation, quotas, policy, audit, and cost attribution — built into the control plane._

- **Multi-Tenancy** — Per-tenant quotas and policy overrides isolate consumers of shared storage. — _Give each team its own guardrails on one physical cluster._
- **Tenant Quotas** — Set and read capacity quotas per tenant, with alerts as usage approaches 80% and 95%. — _Prevent one tenant from starving everyone else._
- **Placement Policies & Drift** — Intent-to-placement policies govern where storage lands, and a policy-drift report flags volumes that no longer match. — _Keep placement compliant with intent over time, not just at creation._
- **Service-Account JWTs & RBAC** — Issue role-scoped JWTs and revoke leaked tokens before their TTL, enforced by RBAC across REST and gRPC. — _Grant least-privilege access and kill compromised tokens instantly._
- **Audit Log** — Every mutating action is recorded, exportable as CSV, with configurable retention-day pruning. — _A defensible trail of who changed what, when._
- **Chargeback** — Attribute cost per tenant from a configurable USD-per-GiB-month rate. — _Turn shared storage into a billable, accountable service._

> Governance controls include a per-minute rate limiter (429 on breach) so a runaway caller can't overwhelm the gateway.

## 8. DataBridge — Database Mobility

_Migrate managed cloud databases to open, self-managed engines on Ceph at the edge._

| Source engine | Edge target | Migration type | Real connector |
|---|---|---|---|
| PostgreSQL | CloudNativePG | homogeneous | default (tokio-postgres) |
| MySQL / MariaDB | Percona XtraDB | homogeneous | default (sqlx) |
| Oracle | CloudNativePG (Postgres) | heterogeneous | feature: oracle (OCI) |
| SQL Server | CloudNativePG (Postgres) | heterogeneous | feature: sqlserver |
| MongoDB | Percona Server for MongoDB | homogeneous (document) | feature: mongodb |

- **Discover & Assess** — Introspect a live source database (tables, PKs, sizes, WAL/CDC capability) and score edge-readiness. — _Know exactly what you're migrating — and its risks — before you start._
- **Edge Provisioning on Ceph** — Provision CloudNativePG, Percona XtraDB, or Percona Server for MongoDB with data and WAL on Ceph RBD. — _Land your database on open engines and your own storage, off the cloud bill._
- **Full-Load + CDC Replication** — Dump-and-restore full load, then Debezium streams ongoing changes (WAL/binlog/redo/oplog) via Kafka to the edge. — _Migrate with the source still live — no big-bang downtime._
- **Guarded Cutover & Rollback** — Cutover is admin-guarded on validation-passed and low CDC lag, with rollback allowed inside a defined window. — _Switch over safely and back out if something looks wrong._
- **Six Source Engines** — PostgreSQL, MySQL, MariaDB, Oracle, SQL Server, and MongoDB — homogeneous and heterogeneous (to Postgres) paths. — _One migration control plane spanning your whole database estate._
- **CDC Self-Heal** — A stalled stream is re-established via cdc/restart, and the reconciler auto-restarts an unhealthy stream up to 3× before alerting. — _Replication recovers itself instead of quietly falling behind._

## 9. DataBridge — Object Migration

_Move AI datasets, model weights, and RAG documents from cloud object stores into Ceph RGW._

- **Cloud-to-Ceph Object Copy** — Copy objects from a cloud store into a Ceph RGW bucket so products consume from a local S3 endpoint. — _Pull AI-scale datasets on-prem and off the cloud egress meter._
- **Incremental, Verified Copy** — List source, diff against destination by key+size, stream each changed object recording sha256, then verify every key landed. — _Trustworthy syncs that only move what changed and prove they arrived._
- **Multi-Cloud Sources** — AWS S3, GCS (S3-interop), and any S3-compatible store work today; Azure Blob has a native feature-gated connector. — _One mover for the object stores you actually use._
- **Concurrent Streaming** — Objects stream and copy concurrently with configurable part size and concurrency for AI-scale datasets. — _Move terabytes fast instead of one slow object at a time._
- **Secret-Ref Credentials** — Credentials are referenced from a Kubernetes Secret, resolved in-process at run time, and never stored or logged. — _Migrate data without spilling access keys into the control plane._

## 10. Disaster Recovery

_Cross-cluster RBD mirroring and failover scaffolding for a second Ceph site._

- **Cluster Peering** — Register and remove peer Ceph clusters as DR targets through the control plane. — _Catalog your DR relationships in one place._
- **Volume Mirroring** — Enable or disable rbd mirror on a volume and list all mirrors with their DR status. — _Keep critical volumes replicated to a second site._
- **Promote & Demote (Failover)** — Promote a mirror at the DR site or demote the primary to orchestrate failover. — _Flip sites when the primary goes down._

> Honest status: the DR control-plane catalog, API, and jobs are complete, but the underlying rbd mirror operations are not yet verified against a live second Ceph cluster — treat DR as scaffolding pending a two-site validation.

## 11. Storage Center Console

_A Zeus OS-style React console embedded in the gateway, wiring every capability to a UI._

- **Embedded React Console** — A React/Vite/Tailwind SPA served from the gateway binary over HTTPS with a branded login. — _Nothing extra to deploy — the UI ships inside Atlas._
- **Full-Capability Coverage** — Inventory, capacity/health, job progress via SSE, alerts, metrics, tenants, and every write action are wired into the UI. — _Drive the whole control plane without touching the CLI._
- **Live Job Progress (SSE)** — Async jobs stream progress to the console over server-sent events as they run. — _Watch provisioning, backups, and migrations advance in real time._
- **Themeable Design** — A Nebula default theme plus Midnight and Aurora variants in the Zeus OS "Tahoe" design language. — _A console that looks like the rest of the Zyvor suite._
- **atlasctl CLI** — A headless REST client covering health, discovery, volumes, snapshots, backups, buckets, RBD, tenants, tokens, and more. — _Script and automate everything the console can do._

## Getting started

1. **Run the gateway (no Ceph, no cluster)** — make run starts the gateway with the fake Ceph driver on 127.0.0.1:5110 — no real storage required.
2. **Populate inventory** — cargo run -p atlas-cli -- discover pulls a normalized inventory from the driver; then pools, volumes, and health are live.
3. **Open the Storage Center** — Browse to http://127.0.0.1:5110/ for the embedded console — inventory, capacity, jobs, alerts, tenants, and write actions.
4. **Try DataBridge fake-first** — make run-databridge runs the whole discover-to-cutover pipeline with no cloud or Kubernetes creds needed.
5. **Go real on k3s + Rook Ceph** — Use deploy/rook-ceph-lab and scripts/deploy-remote.sh to stand up real Ceph and deploy the gateway in Ceph mode.

> **Good to know:** Atlas is at slices 1–5 plus DataBridge, verified end-to-end on real k3s + Rook Ceph clusters. Some capabilities require real infrastructure or optional build features: cross-cluster DR (rbd mirror) is complete as control-plane scaffolding but unverified without a live second Ceph cluster; true multi-replica HA requires Postgres (single-replica SQLite is the default); Oracle, SQL Server, MongoDB, precise CDC lag, and Azure Blob connectors are behind cargo features that link native dependencies; and OSD/QoS/pool-migration and live CDC for non-Postgres engines exercise real Ceph and a running Kafka/Debezium stack respectively.

---
_Atlas is developed by ZyvorAI Labs. Contact **info@zyvor.dev** · Proprietary & Confidential._
