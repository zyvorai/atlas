<!-- Copyright (c) 2026 ZyvorAI Labs Private Limited. All rights reserved. -->
# Contributing to Atlas

## Conventions

- **License header** on every source file:
  `// Copyright (c) 2026 ZyvorAI Labs Private Limited. All rights reserved.`
  (`#` / `<!-- -->` variants for shell/yaml/markdown). Each `Cargo.toml` sets
  `license = "LicenseRef-Zyvor-Proprietary"`.
- **Stack**: axum 0.8, `sqlx` (SQLite), `thiserror` 2.0 + `anyhow`, `tracing`, `kube-rs`.
  Match the surrounding monorepo services (`ragnarok`, `machina`).
- **Errors**: return `atlas_common::AppError` from handlers; it maps to HTTP via `IntoResponse`.
  Library crates return `anyhow::Result` / typed errors and convert at the edge.
- **Ceph/rbd commands**: build arguments as **arrays only**, never string concatenation
  (`ceph_cmd(&["df", "detail"])`). Always `--format json`.
- **Secrets**: store *references*, never values. Never print a keyring/secret to logs or output.
- Run `make dev` (fmt + clippy `-D warnings` + tests) before committing.

## Adding a REST endpoint

1. Add the handler in `crates/atlas-gateway/src/routes.rs` (return `AppResult<Json<...>>`).
2. Wire the route in `router()`.
3. Read data via `atlas-inventory` (SQLite) or a driver; don't touch backends directly from the
   handler.
4. If it's state-changing, write an audit row (`atlas_inventory::audit::record`).
5. Add/extend a test in `crates/atlas-gateway/tests/read_only.rs`.
6. Document it in `docs/API.md`.

## Adding a storage backend driver

1. New crate `crates/atlas-driver-<name>`; implement `atlas_driver_core::StorageDriver`.
2. Map the backend's native objects into `atlas-api-types` DTOs (add fields there if needed —
   that crate is the shared contract).
3. Register it in `crates/atlas-gateway/src/startup.rs` (`DriverRegistry`).
4. Keep read methods live; leave write methods as `NotImplemented` until the write slice.
5. Unit-test the parsing/normalization (see `atlas-driver-ceph`'s `real.rs` tests).

## Adding a database table / migration

- Add a new numbered file in `migrations/` (e.g. `0002_jobs.sql`), SQLite dialect.
  Never edit an already-shipped migration.
- SQLite mappings: timestamps → `TEXT` RFC3339 (`strftime('%Y-%m-%dT%H:%M:%fZ','now')`),
  JSON → `TEXT` + `CHECK(json_valid(col))`, auto-id → `INTEGER PRIMARY KEY AUTOINCREMENT`.
- Add upsert/read helpers in `atlas-inventory`.

## Tests

- Unit tests live beside the code (`#[cfg(test)] mod tests`).
- Gateway integration tests spin up the server with the **fake** driver + a throwaway SQLite
  file (`enable_k8s: false`, `initial_discovery: false`) — no external services required.

## Commits

- Keep commits focused; describe the *why*. End with:
  `Co-Authored-By: ...` where applicable.
- Don't commit `atlas.db*`, `target/`, or `.env` (see `.gitignore`).
