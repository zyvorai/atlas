<!-- Copyright (c) 2026 ZyvorAI Labs Private Limited. All rights reserved. -->
# atlas-inventory

The SQLite-backed normalized inventory read model + audit log.

- **`connect` / `migrate`** — open the pool (WAL, foreign keys on, `create_if_missing`) and run
  the embedded migrations from the workspace `migrations/` dir. Mirrors
  `machina/controller/src/db/mod.rs`.
- **Writes** — `upsert_backend`, `upsert_discovery` (one transaction: cluster → pools → osds →
  volumes, with enum↔TEXT mapping).
- **Reads** — `list_backends`, `list_clusters`, `get_cluster`, `cluster_health`, `list_pools`,
  `list_osds`, `list_volumes`, `get_volume`, `metrics_summary`. These return `atlas-api-types`
  DTOs and power the gateway's read endpoints.
- **`audit`** — `record(...)` appends to `storage_audit_logs`; `count_for_action` (used in tests).

Schema: `migrations/0001_init.sql`. Uses runtime `sqlx::query`/`query_as` (no compile-time
`DATABASE_URL` needed).
