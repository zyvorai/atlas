// Copyright (c) 2026 ZyvorAI Labs Private Limited.
// SPDX-License-Identifier: AGPL-3.0-only OR LicenseRef-Atlas-Commercial
//! Live verification for the Phase-1 HA scaffolding (docs/HA.md): proves connect_postgres() and
//! migrate_postgres() work against a real Postgres, not just that they compile behind the
//! `postgres` feature. Does NOT prove the query layer runs on Postgres — that's still SQLite-only
//! and explicitly out of scope (see deploy/postgres-lab/README.md).
//!
//! Opt-in (depends on external infra + the `postgres` feature, `cargo test` doesn't run
//! `#[ignore]`d tests by default):
//!   deploy/postgres-lab/up.sh   # stands up a throwaway Postgres in the lab
//!   DATABASE_URL="postgres://atlas:<password>@<node-ip>:30432/atlas" \
//!     cargo test -p atlas-inventory --features postgres --test postgres_live -- --ignored --nocapture

#![cfg(feature = "postgres")]

use sqlx::Row;

fn lab_database_url() -> String {
    std::env::var("DATABASE_URL").expect(
        "set DATABASE_URL to the lab Postgres, e.g. \
         postgres://atlas:<password>@<node-ip>:30432/atlas — see deploy/postgres-lab/README.md \
         for how to fetch the generated password",
    )
}

#[tokio::test]
#[ignore]
async fn connect_and_migrate_against_real_postgres() {
    let url = lab_database_url();

    let pool = atlas_inventory::connect_postgres(&url)
        .await
        .expect("connect_postgres should open a real connection to the lab Postgres");

    atlas_inventory::migrate_postgres(&pool)
        .await
        .expect("migrate_postgres should apply migrations-postgres/ cleanly");

    // Re-running migrate is idempotent (sqlx tracks applied versions in _sqlx_migrations) — this
    // is what a pod restart / rolling redeploy does on every boot in the real code path.
    atlas_inventory::migrate_postgres(&pool)
        .await
        .expect("migrate_postgres must be safe to run again against an already-migrated database");

    // Spot-check a handful of tables spanning the migration history, including the three ported
    // in this session (rbd_snapshots, the native-id unique index, console_users.tenant_id) — a
    // schema-parity gap here would mean migrations-postgres/ silently fell behind migrations/
    // again.
    for table in ["storage_volumes", "storage_audit_logs", "rbd_snapshots", "console_users"] {
        let row = sqlx::query(&format!("SELECT COUNT(*) AS c FROM {table}"))
            .fetch_one(&pool)
            .await
            .unwrap_or_else(|e| panic!("table {table} should exist and be queryable: {e}"));
        let _: i64 = row.get("c");
    }

    let tenant_col_exists: bool = sqlx::query(
        "SELECT EXISTS (SELECT 1 FROM information_schema.columns \
         WHERE table_name = 'console_users' AND column_name = 'tenant_id')",
    )
    .fetch_one(&pool)
    .await
    .unwrap()
    .get(0);
    assert!(tenant_col_exists, "console_users.tenant_id must exist (migration 0028)");

    pool.close().await;
}
