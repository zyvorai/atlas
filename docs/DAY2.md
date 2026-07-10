<!-- Copyright (c) 2026 ZyvorAI Labs Private Limited. All rights reserved. -->
# Atlas Day-2 Operations

Atlas is an *operable* control plane, not just observe-and-provision. This is the operator runbook for
the day-2 capabilities; see `docs/API.md` for full request/response detail. Everything below is
fake-first tested + CI-green; where a capability needs real infra to fully verify, it's called out.

## Control-plane durability & health
- **Survives restarts**: on boot the job engine recovers — an interrupted `running` job is failed-safe
  (never stuck), `queued`/`pending` jobs re-enqueued. Opt-in bounded retry per job.
- **Graceful shutdown**: `SIGTERM` drains in-flight REST + gRPC before exit (`terminationGracePeriodSeconds: 30`).
- **Probes**: `GET /livez` (liveness → restart) vs `GET /readyz` (deep: DB + real driver probe + worker
  heartbeats → depool). Wire k8s livenessProbe→`/livez`, readinessProbe→`/readyz`.
- **Self-state backup**: `ATLAS_STATE_BACKUP_SECS` + `_ENDPOINT`/`_BUCKET`/`_ACCESS_KEY`/`_SECRET_KEY`
  (`_KEEP`, default 24) → `VACUUM INTO` snapshots of the control-plane DB to S3/RGW. Off by default.
- **HA**: a DB leader lease (`leader_lease`) gates the periodic workers so only one replica schedules.
  Single-replica SQLite always wins; **true multi-replica needs Postgres** (`ATLAS_DATABASE_URL`).

## Alerting (operator)
Rules: cluster health, pool near-full (75/85%), OSD down, capacity forecast, OSD latency, recovery,
**jobs failing (15m), CDC replication error, tenant quota approaching (80/95%)**. Lifecycle:
- `POST /alerts/{id}/ack` · `/silence[?secs]` (suppress webhook for a window) · `/resolve`.
- One webhook sink via `ATLAS_ALERT_WEBHOOK_URL`.

## Maintenance & cluster ops (admin)
- **Cordon** `POST /backends/{id}/cordon` · `/uncordon` — a cordoned backend rejects new provisioning (503).
- **Pause** `GET`/`POST /maintenance {paused}` — the job worker holds jobs (`queued`) until resumed.
- **OSD ops** `POST /osds/{id}/out|in|reweight?weight=` (`ceph osd …`). *Real Ceph.*
- **Dynamic backends** `POST /backends {backend_type:"nfs"|"zfs", server, targets}` — instantiates a live
  driver + discovers it immediately (not just a catalog row).

## Volume lifecycle
- **QoS** `POST /rbd-images/{pool}/{image}/qos?iops=&bps=` (0 clears). *Real Ceph.*
- **Resize-down** `POST …/resize {size_bytes, allow_shrink:true}` — shrink is opt-in (data-loss guard).
- **Pool migration** `POST …/migrate?dest_pool=` (`rbd migration`). *Real Ceph.*
- **Orphan GC** `GET /maintenance/orphans` — backups whose source volume is gone; clean via `DELETE /backups/{id}`.

## Governance (admin/operator)
- **Token revocation** `POST /auth/tokens/{jti}/revoke` + `GET /auth/tokens/revoked` — kill a leaked
  token before its TTL. **Rate limiting** `ATLAS_RATE_LIMIT_RPM` (0=off) → 429.
- **Audit** `GET /audit.csv` export; `ATLAS_AUDIT_RETENTION_DAYS` prunes old rows.
- **Chargeback** `GET /chargeback` (`ATLAS_CHARGEBACK_USD_PER_GIB_MONTH`). **Policy drift** `GET /policy-drift`.

## DataBridge day-2
- CDC self-heal: `POST /databridge/plans/{id}/cdc/restart` re-establishes a stalled stream; the
  reconciler also auto-restarts an unhealthy stream up to 3× before giving up (→ `error` → alert).

## Cross-cluster DR (admin) — scaffolding
Control-plane catalog + API + jobs are here; the real `rbd mirror` operations are **UNVERIFIED** without
a live second Ceph cluster. `POST/GET /dr/peers`; `POST/DELETE /volumes/{id}/mirror`; `GET /dr/mirrors`
+ `/dr/status`; `POST /dr/mirrors/{id}/promote|demote` (failover).

## Upgrades
- **Pre-flight** `GET /upgrade/preflight` — no HEALTH_ERR cluster / open critical alerts / in-flight
  jobs / lagging CDC → `{ ready, blockers }`. `scripts/deploy-remote.sh` gates on it and supports
  `--rollback` (`kubectl rollout undo`) + `--force`.

## Standard procedures
- **Rolling upgrade**: `GET /upgrade/preflight` → if `ready:false`, `POST /maintenance {paused:true}`
  and let jobs drain → `scripts/deploy-remote.sh <host> <user>` (auto-gates + rollout-restarts) →
  `POST /maintenance {paused:false}`. Bad upgrade → `scripts/deploy-remote.sh <host> <user> --rollback`.
- **Backend maintenance**: `POST /backends/{id}/cordon` → drain → do the work → `/uncordon`.
- **Leaked credential**: `POST /auth/tokens/{jti}/revoke` (find the jti via the issue response or audit).
- **Stalled migration**: alert fires on `CDC replication error` → `POST …/cdc/restart` (or wait for the
  reconciler's bounded auto-restart).
