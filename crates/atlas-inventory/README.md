<!-- Copyright (c) 2026 ZyvorAI Labs Private Limited. -->
<!-- SPDX-License-Identifier: AGPL-3.0-only OR LicenseRef-Atlas-Commercial -->
# atlas-inventory

The SQLite-backed normalized inventory read model + audit log.

- **`connect` / `connect_sqlite` / `migrate`** — open the SQLite pool (WAL, foreign keys on,
  `create_if_missing`) and run the embedded migrations from the workspace `migrations/` dir.
  Postgres URLs are rejected with a pointer to `docs/HA.md`.
- **`postgres` feature** — Phase-1 HA: `connect_postgres` + `migrate_postgres` against
  `migrations-postgres/` (`sqlx::PgPool`). Does not port inventory queries off SQLite yet.
- **Writes** — `upsert_backend`, `upsert_discovery` (one transaction: cluster → pools → osds →
  volumes, with enum↔TEXT mapping).
- **Reads** — `list_backends`, `list_clusters`, `get_cluster`, `cluster_health`, `list_pools`,
  `list_osds`, `list_volumes`, `get_volume`, `metrics_summary`. These return `atlas-api-types`
  DTOs and power the gateway's read endpoints.
- **`audit`** — `record(...)` appends to `storage_audit_logs`; `count_for_action` (used in tests).

Schema: `migrations/0001_init.sql` (SQLite). Postgres mirror: `migrations-postgres/`.
Uses runtime `sqlx::query`/`query_as` (no compile-time `DATABASE_URL` needed).
