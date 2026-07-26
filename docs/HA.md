<!-- Copyright (c) 2026 ZyvorAI Labs Private Limited. All rights reserved. -->
# High availability foundation

Atlas today runs as a **single replica** with SQLite on a ReadWriteOnce PVC. True multi-replica HA
needs a shared database (PostgreSQL). This document describes what is already durable, what still
blocks multi-replica, and the cutover plan.

## What is durable now

| Piece | Behavior |
|---|---|
| **Job rows** | Every write is a `storage_jobs` row. The in-memory channel is only a wake-up. |
| **DB poller** | `ATLAS_JOB_POLL_SECS` (default `2`) scans for due `queued`/`pending` work and wakes the worker. Survives channel loss and process restarts. |
| **Retry backoff** | `next_attempt_at` is honored by recovery + poller (jobs are not fired early after a crash). |
| **Atomic claim** | `try_claim` stamps `locked_by` / `locked_at` and flips state to `running` so channel + poller races cannot double-execute. |
| **Stale reclaim** | `ATLAS_JOB_STALE_SECS` (default `900`) re-queues `running` jobs whose lock is older than the threshold (hard kill without boot recovery). |
| **Boot recovery** | Interrupted `running` → `failed` (fail-safe); due queued work is re-enqueued. |
| **Leader lease** | `leader_lease` table gates monitor / scheduler / DataBridge reconciler so only one holder schedules. |

## What still blocks multi-replica

1. **SQLite + RWO PVC** — cannot mount on two pods; Deployments use `strategy: Recreate`.
2. **sqlx queries** — inventory/jobs use SQLite placeholders (`?`) and `SqlitePool` throughout.
3. **In-process rate limiter** — per-pod fixed windows (`ATLAS_RATE_LIMIT_RPM`).
4. **Ceph CLI / local `/etc/ceph`** — real driver assumes local credentials rendered into the pod.

## PostgreSQL cutover (planned)

Lab Postgres for development (does **not** switch Atlas yet):

```bash
docker compose -f deploy/postgres/docker-compose.yml up -d
# Connection string for the future cutover:
# ATLAS_DATABASE_URL=postgres://atlas:atlas@127.0.0.1:5432/atlas
```

Today, a `postgres://` / `postgresql://` `ATLAS_DATABASE_URL` is **rejected at connect** with a pointer
here — so misconfiguration fails loudly instead of half-working.

Remaining work for the cutover:

1. Translate `migrations/*.sql` to Postgres (types, `strftime` → `now()`, `json_set`, etc.).
2. Introduce a sqlx backend feature (`sqlite` | `postgres`) or `sqlx::Any` and dual query modules.
3. Raise pool size; keep leader lease + job claim semantics unchanged.
4. Move rate limiting to Redis or DB-backed counters.
5. Change Deployments to `RollingUpdate` + `ReadWriteMany`/no local DB volume.

## Config knobs

| Env | Default | Meaning |
|---|---|---|
| `ATLAS_DATABASE_URL` | `sqlite://atlas.db?mode=rwc` | SQLite only for now; Postgres URL refused |
| `ATLAS_JOB_POLL_SECS` | `2` | Durable queue poll interval (`0` disables; tests use `0`) |
| `ATLAS_JOB_STALE_SECS` | `900` | Reclaim stale `running` locks (`0` disables) |
