# Atlas — Complete page index

Every primary navigable dashboard route.

_Generated: 2026-07-26 · 28 routes_

Regenerate: `node scripts/customer-docs/generate-page-index.mjs`

## STORAGE

| Page | Route | Purpose | Guide |
|------|-------|---------|-------|
| Command Deck | `/` | Storage Center home — capacity, job health, backend status, and shortcuts into volumes and protection. | [Open](pages/storage/home.md) |
| Volumes | `/volumes` | Intent-backed volume inventory across registered storage backends. | [Open](pages/storage/volumes.md) |
| RBD Images | `/rbd` | Ceph RBD image inventory when the Ceph driver is active. | [Open](pages/storage/rbd.md) |
| Snapshots | `/snapshots` | Volume and image snapshots for point-in-time recovery. | [Open](pages/storage/snapshots.md) |
| Schedules | `/schedules` | Snapshot / backup schedule policies. | [Open](pages/storage/schedules.md) |

## DATA PROTECTION

| Page | Route | Purpose | Guide |
|------|-------|---------|-------|
| Backups | `/backups` | Backup jobs and restore entry points for protected volumes. | [Open](pages/data-protection/backups.md) |
| Buckets | `/buckets` | Object / RGW bucket inventory and lifecycle. | [Open](pages/data-protection/buckets.md) |

## OBSERVABILITY

| Page | Route | Purpose | Guide |
|------|-------|---------|-------|
| Observatory | `/observatory` | Cross-cutting health and capacity observatory for the storage plane. | [Open](pages/observability/observatory.md) |
| Activity | `/activity` | Recent storage activity feed (provisions, deletes, migrations). | [Open](pages/observability/activity.md) |
| Alerts | `/alerts` | Storage and backend alerts requiring operator attention. | [Open](pages/observability/alerts.md) |
| Metrics | `/metrics-dashboard` | Metrics dashboards for pools, devices, and gateway latency. | [Open](pages/observability/metrics-dashboard.md) |
| Jobs | `/jobs` | Durable async jobs from every mutating Atlas API call. | [Open](pages/observability/jobs.md) |
| Audit | `/audit` | Audit log of storage control-plane actions (exportable). | [Open](pages/observability/audit.md) |

## GOVERNANCE

| Page | Route | Purpose | Guide |
|------|-------|---------|-------|
| Tenants | `/tenants` | Multi-tenant isolation and quota boundaries. | [Open](pages/governance/tenants.md) |
| Access | `/access` | Tokens, roles, and API access for Storage Center and clients. | [Open](pages/governance/access.md) |
| Policies | `/policies` | Storage intent policies (production, database, development, shared, ai). | [Open](pages/governance/policies.md) |

## DATABRIDGE

| Page | Route | Purpose | Guide |
|------|-------|---------|-------|
| Cloud Databases | `/databridge/sources` | Cloud / external database sources for DataBridge migrations. | [Open](pages/databridge/databridge-sources.md) |
| Migration Plans | `/databridge/plans` | Database and object migration plans. | [Open](pages/databridge/databridge-plans.md) |
| Plan Detail | `/databridge/plans/:id` | Single migration plan detail — stages, validation, and cutover. | [Open](pages/databridge/databridge-plans-:id.md) |
| Edge DB Clusters | `/databridge/edge-clusters` | Edge database clusters managed through DataBridge. | [Open](pages/databridge/databridge-edge-clusters.md) |
| Replication | `/databridge/replication` | Ongoing replication links for DataBridge workloads. | [Open](pages/databridge/databridge-replication.md) |
| Validation | `/databridge/validation` | Pre/post migration validation results. | [Open](pages/databridge/databridge-validation.md) |

## INFRASTRUCTURE

| Page | Route | Purpose | Guide |
|------|-------|---------|-------|
| Backends | `/backends` | Registered StorageDriver backends (Ceph first; NFS/ZFS/etc. as added). | [Open](pages/infrastructure/backends.md) |
| Kubernetes | `/kubernetes` | Kubernetes StorageClass / CSI integration status for Atlas. | [Open](pages/infrastructure/kubernetes.md) |
| Cluster | `/cluster` | Atlas gateway cluster / HA membership view. | [Open](pages/infrastructure/cluster.md) |
| Ceph | `/ceph` | Ceph-specific day-2 operations (OSD, pool, RGW health). | [Open](pages/infrastructure/ceph.md) |
| Maintenance | `/maintenance` | Maintenance windows and safe ops against storage backends. | [Open](pages/infrastructure/maintenance.md) |
| Disaster Recovery | `/dr` | Disaster recovery plans and failover for Atlas-managed data. | [Open](pages/infrastructure/dr.md) |

## Related

- [Customer docs home](README.md)
- [Page-by-page guides](pages/README.md)
