<!-- Copyright (c) 2026 ZyvorAI Labs Private Limited. -->
<!-- SPDX-License-Identifier: AGPL-3.0-only OR LicenseRef-Atlas-Commercial -->
# High availability foundation

Atlas today runs as a **single replica** with SQLite on a ReadWriteOnce PVC. True multi-replica HA
needs a shared database (PostgreSQL). This document describes what is already durable, what still
blocks multi-replica, and the cutover plan.

## Phase-1 status (foundation landed)

Lab and gateway **remain on SQLite** — nothing flips `ATLAS_DATABASE_URL` in deploy.

| Landed | Detail |
|---|---|
| **`migrations-postgres/`** | Postgres dialect of `migrations/` 0001..0025 (`PRAGMA` removed, `AUTOINCREMENT` → `BIGSERIAL`, `strftime` → `to_char` UTC TEXT defaults, `json_valid` → `::json` CHECKs). Same table/column names. |
| **`atlas-inventory` feature `postgres`** | Enables `sqlx/postgres`. Adds `connect_postgres` + `migrate_postgres` (runs `migrations-postgres` via `sqlx::migrate!`). Default SQLite `connect` / `migrate` unchanged. |
| **Smoke** | `scripts/smoke-postgres-ha.sh` and `crates/atlas-inventory/tests/postgres_connect.rs` (live migrate is `#[ignore]` — needs `DATABASE_URL`). |
| **K8s sketch** | `deploy/k8s/atlas-postgres.yaml` (plain Deployment + optional CNPG comment) — **not applied** to lab. |

Remaining for a full cutover (not Phase-1):

1. Dual query modules (or `sqlx::Any`) — inventory/jobs still use SQLite `?` placeholders and `SqlitePool` end-to-end.
2. Gateway/job engine wired to `PgPool` when URL is Postgres; raise pool size; keep leader lease + job claim semantics.
3. Move rate limiting to Redis or DB-backed counters.
4. Deployments → `RollingUpdate` + no local DB volume / RWX as needed.
5. Console `COLLATE NOCASE` parity (`citext` or lower() unique index).

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

## PostgreSQL cutover

Lab Postgres for development (does **not** switch Atlas yet):

```bash
docker compose -f deploy/postgres/docker-compose.yml up -d
# Connection string for Phase-1 migrate smoke:
# DATABASE_URL=postgres://atlas:atlas@127.0.0.1:5432/atlas
./scripts/smoke-postgres-ha.sh --migrate
```

Build / check the optional feature:

```bash
cargo check -p atlas-inventory --features postgres
```

Default SQLite `connect("postgres://...")` still **rejects** with a pointer here so a mis-set
`ATLAS_DATABASE_URL` fails loudly. Use `connect_postgres` only behind the `postgres` feature.

## Config knobs

| Env | Default | Meaning |
|---|---|---|
| `ATLAS_DATABASE_URL` | `sqlite://atlas.db?mode=rwc` | SQLite for lab/gateway; Postgres URL refused by default `connect` |
| `DATABASE_URL` | — | Used by Phase-1 smoke / ignored migrate test |
| `ATLAS_JOB_POLL_SECS` | `2` | Durable queue poll interval (`0` disables; tests use `0`) |
| `ATLAS_JOB_STALE_SECS` | `900` | Reclaim stale `running` locks (`0` disables) |
