# Page-by-page guides

Each guide follows: Purpose → When to use it → How to get there → What you can do → Related pages.

Every route is also listed in the [complete page index](../PAGE_INDEX.md).

## Data Protection

| Page | What it covers |
|------|----------------|
| [Backups](data-protection/backups.md) | Backup jobs and restore entry points for protected volumes. |
| [Buckets](data-protection/buckets.md) | Object / RGW bucket inventory and lifecycle. |

## Databridge

| Page | What it covers |
|------|----------------|
| [Edge DB Clusters](databridge/databridge-edge-clusters.md) | Edge database clusters managed through DataBridge. |
| [Plan Detail](databridge/databridge-plans-:id.md) | Single migration plan detail — stages, validation, and cutover. |
| [Migration Plans](databridge/databridge-plans.md) | Database and object migration plans. |
| [Replication](databridge/databridge-replication.md) | Ongoing replication links for DataBridge workloads. |
| [Cloud Databases](databridge/databridge-sources.md) | Cloud / external database sources for DataBridge migrations. |
| [Validation](databridge/databridge-validation.md) | Pre/post migration validation results. |

## Governance

| Page | What it covers |
|------|----------------|
| [Access](governance/access.md) | Tokens, roles, and API access for Storage Center and clients. |
| [Policies](governance/policies.md) | Storage intent policies (production, database, development, shared, ai). |
| [Tenants](governance/tenants.md) | Multi-tenant isolation and quota boundaries. |

## Infrastructure

| Page | What it covers |
|------|----------------|
| [Backends](infrastructure/backends.md) | Registered StorageDriver backends (Ceph first; NFS/ZFS/etc. as added). |
| [Ceph](infrastructure/ceph.md) | Ceph-specific day-2 operations (OSD, pool, RGW health). |
| [Cluster](infrastructure/cluster.md) | Atlas gateway cluster / HA membership view. |
| [Disaster Recovery](infrastructure/dr.md) | Disaster recovery plans and failover for Atlas-managed data. |
| [Kubernetes](infrastructure/kubernetes.md) | Kubernetes StorageClass / CSI integration status for Atlas. |
| [Maintenance](infrastructure/maintenance.md) | Maintenance windows and safe ops against storage backends. |

## Observability

| Page | What it covers |
|------|----------------|
| [Activity](observability/activity.md) | Recent storage activity feed (provisions, deletes, migrations). |
| [Alerts](observability/alerts.md) | Storage and backend alerts requiring operator attention. |
| [Audit](observability/audit.md) | Audit log of storage control-plane actions (exportable). |
| [Jobs](observability/jobs.md) | Durable async jobs from every mutating Atlas API call. |
| [Metrics](observability/metrics-dashboard.md) | Metrics dashboards for pools, devices, and gateway latency. |
| [Observatory](observability/observatory.md) | Cross-cutting health and capacity observatory for the storage plane. |

## Storage

| Page | What it covers |
|------|----------------|
| [Command Deck](storage/home.md) | Storage Center home — capacity, job health, backend status, and shortcuts into volumes and protection. |
| [RBD Images](storage/rbd.md) | Ceph RBD image inventory when the Ceph driver is active. |
| [Schedules](storage/schedules.md) | Snapshot / backup schedule policies. |
| [Snapshots](storage/snapshots.md) | Volume and image snapshots for point-in-time recovery. |
| [Volumes](storage/volumes.md) | Intent-backed volume inventory across registered storage backends. |

---

28 guides. Regenerate: `node scripts/customer-docs/generate-guide-index.mjs`.
