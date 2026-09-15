<!-- Copyright (c) 2026 ZyvorAI Labs Private Limited. -->
<!-- SPDX-License-Identifier: AGPL-3.0-only OR LicenseRef-Atlas-Commercial -->
# High availability foundation

Atlas has historically run as a **single replica** with SQLite on a ReadWriteOnce PVC. True
multi-replica HA needs a shared database (PostgreSQL). This document describes what has landed,
what's proven, and what's left for a production multi-replica cutover.

## Status: query layer is backend-agnostic (Phases A + B landed)

Every crate now builds against `sqlx::AnyPool` and connects to either SQLite or Postgres at
runtime, chosen by `ATLAS_DATABASE_URL`'s scheme — there is no more compile-time `postgres`
Cargo feature. Lab/gateway **default to SQLite**; nothing in `deploy/` flips the URL yet, so this
is not a behavior change for existing deployments — it's the query layer becoming capable of
running on Postgres in the same binary.

| Landed | Detail |
|---|---|
| **`migrations-postgres/`** | Postgres dialect of `migrations/`, kept 1:1 in lockstep (`scripts/check-migrations-parity.sh`, wired into CI — fails the build if the two directories' migration numbers ever diverge, which happened silently twice before this check existed). |
| **`atlas_inventory::connect()`/`migrate()`** | One function each, no feature flag. `connect()` uses `sqlx::any::install_default_drivers()` + `AnyPoolOptions`, auto-detecting SQLite vs Postgres from the URL scheme; SQLite-only pragmas (WAL, `foreign_keys`, `busy_timeout`, `synchronous`) apply via an `after_connect` hook, skipped for Postgres. `migrate()` scheme-dispatches to `migrations/` or `migrations-postgres/`. |
| **Query portability cleanup** | All ~250 `sqlx::query` call sites across `atlas-inventory`/`atlas-jobs`/`atlas-monitor`/`atlas-gateway`/`atlas-databridge` (the DataBridge control-plane tables, not customer source-DB connections) use `$N` placeholders (both backends' drivers accept these identically — confirmed live, `sqlx::Any` does zero placeholder rewriting itself). `strftime('now', ...)` (~80 sites) replaced with a Rust-bound `chrono` timestamp via `now_rfc3339()`; `INSERT OR IGNORE` → `ON CONFLICT DO NOTHING`; SQLite's `COLLATE NOCASE` → `lower(x) = lower($N)` (with a matching expression index in both migration directories); SQLite's 2-argument scalar `MAX(a, b)` (Postgres's `MAX()` is aggregate-only) → `CASE WHEN a > b THEN a ELSE b END`. |
| **State backup** | `spawn_state_backup` branches on backend: SQLite keeps `VACUUM INTO`; Postgres shells out to `pg_dump --format=custom` (mirrors the existing pattern of shelling out to the `ceph`/`rbd` CLI — the gateway image needs `pg_dump` on `PATH` when `ATLAS_DATABASE_URL` is a Postgres URL). |
| **Live-verified against a real Postgres** | `crates/atlas-inventory/tests/postgres_live.rs` (`#[ignore]`, needs `DATABASE_URL`) exercises connect+migrate+schema parity, the job-claim/reclaim/retry state machine in `jobs.rs` (where the `MAX(a,b)` bug was actually found), and `users.rs`'s case-insensitive username lookup — not just that the code compiles against Postgres. Run via `./scripts/smoke-postgres-ha.sh --migrate`. |
| **Helm chart `database.kind`** | `deploy/helm/atlas/values.yaml`'s new `database.kind: sqlite\|postgres` (+ `database.existingSecret`/`secretKey`) switches `templates/deployment.yaml`/`pvc.yaml` between the default single-replica shape (local PVC, `Recreate`) and a Postgres-backed one (`ATLAS_DATABASE_URL` from a Secret, no PVC rendered, `RollingUpdate`, `replicaCount` free to raise). See `deploy/helm/atlas/README.md`'s "Multi-replica (Postgres-backed) deployment". |

Remaining for a full multi-replica cutover (Phase C, plus one Phase D item):

1. A parameterized test-spawn helper so the ~200 existing `atlas-gateway` integration tests run
   against both backends in CI (a `postgres:16` service container), not just this crate's own
   live-Postgres tests.
2. `deploy/postgres-lab/` gets a real smoke test beyond connect+migrate (apply migrations, then
   exercise real read/write flows).
3. **DB-backed rate limiting — blocked on a real architectural constraint, not just unstarted
   work.** `RateLimiter::allow()` (`crates/atlas-gateway/src/state.rs`) is called from two places:
   the REST `axum` middleware (`auth.rs`, already `async`, an `AnyPool` query would be easy there)
   and the gRPC `tonic::Interceptor` closure (`grpc.rs`), whose trait signature is **synchronous**
   (`fn call(&mut self, req) -> Result<Request<()>, Status>`, no `.await`). Blocking that closure on
   an async DB query risks deadlocking the Tokio runtime it's invoked from, or at minimum serializes
   every gRPC call behind a blocking DB round-trip. A real fix needs either a tonic interceptor
   redesign (there's no drop-in async variant of the closure-based API used here) or a
   write-behind/cached-count approach that tolerates being slightly stale across replicas — worth
   scoping as its own `/plan`, not a mechanical follow-on to this migration. Until then, per-pod
   in-process rate limiting means `replicaCount` pods each get their own independent
   `ATLAS_RATE_LIMIT_RPM` budget rather than sharing one cluster-wide window — an acceptable,
   documented gap for a first multi-replica cutover, not a correctness bug.
4. Ceph CLI/local `/etc/ceph` credentials — **already not blocking**: the real-Ceph Deployment
   renders `/etc/ceph` per-pod via its own `initContainer` (see the Helm chart), so this item from
   an earlier version of this doc no longer applies.

## What is durable now

| Piece | Behavior |
|---|---|
| **Job rows** | Every write is a `storage_jobs` row. The in-memory channel is only a wake-up. |
| **DB poller** | `ATLAS_JOB_POLL_SECS` (default `2`) scans for due `queued`/`pending` work and wakes the worker. Survives channel loss and process restarts. |
| **Retry backoff** | `next_attempt_at` is honored by recovery + poller (jobs are not fired early after a crash). |
| **Atomic claim** | `try_claim` stamps `locked_by` / `locked_at` and flips state to `running` so channel + poller races cannot double-execute. Verified on real Postgres, not just SQLite. |
| **Stale reclaim** | `ATLAS_JOB_STALE_SECS` (default `900`) re-queues `running` jobs whose lock is older than the threshold (hard kill without boot recovery). |
| **Boot recovery** | Interrupted `running` → `failed` (fail-safe); due queued work is re-enqueued. |
| **Leader lease** | `leader_lease` table gates monitor / scheduler / DataBridge reconciler so only one holder schedules. Design was already portable (an atomic conditional `ON CONFLICT ... DO UPDATE ... WHERE` upsert) — only needed dialect translation, not a redesign. |

## What still blocks multi-replica

1. **SQLite + RWO PVC on the default install** — the chart still defaults to `database.kind:
   sqlite`. Set `database.kind: postgres` (see `deploy/helm/atlas/README.md`) to lift this — the
   query layer itself is ready (see "Live-verified" above) and the chart now renders
   `RollingUpdate` + no PVC in that mode.
2. **In-process rate limiter** — per-pod fixed windows (`ATLAS_RATE_LIMIT_RPM`), not shared across
   replicas. Blocked on a real constraint (the gRPC `tonic::Interceptor` is synchronous), not just
   unstarted work — see item 3 above for the detail.

## PostgreSQL cutover

Lab Postgres for development (does **not** switch Atlas's own deploy manifests yet):

```bash
deploy/postgres-lab/up.sh   # or: docker compose -f deploy/postgres/docker-compose.yml up -d
export DATABASE_URL=postgres://atlas:<password>@<host>:<port>/atlas
./scripts/smoke-postgres-ha.sh --migrate
```

To actually run the gateway against Postgres locally:

```bash
ATLAS_DATABASE_URL="postgres://atlas:<password>@<host>:<port>/atlas" cargo run -p atlas-gateway
```

`connect()` auto-detects the backend from the URL scheme — no feature flag, no code change.

## Config knobs

| Env | Default | Meaning |
|---|---|---|
| `ATLAS_DATABASE_URL` | `sqlite://atlas.db?mode=rwc` | SQLite or Postgres URL — backend auto-detected by `connect()` |
| `DATABASE_URL` | — | Used by `scripts/smoke-postgres-ha.sh` / the `#[ignore]`d live Postgres tests |
| `ATLAS_JOB_POLL_SECS` | `2` | Durable queue poll interval (`0` disables; tests use `0`) |
| `ATLAS_JOB_STALE_SECS` | `900` | Reclaim stale `running` locks (`0` disables) |
| `ATLAS_STATE_BACKUP_SECS` | `0` (disabled) | Periodic self-state backup interval; snapshot mechanism (`VACUUM INTO` vs `pg_dump`) auto-selected by backend |
