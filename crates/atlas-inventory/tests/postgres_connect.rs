// Copyright (c) 2026 ZyvorAI Labs Private Limited. All rights reserved.
//! Phase-1 HA smoke: connect + migrate against Postgres when `DATABASE_URL` is set.
//!
//! ```bash
//! # optional lab DB:
//! docker compose -f deploy/postgres/docker-compose.yml up -d
//! DATABASE_URL=postgres://atlas:atlas@127.0.0.1:5432/atlas \
//!   cargo test -p atlas-inventory --features postgres --test postgres_connect -- --ignored
//! ```
//!
//! Without Docker / `DATABASE_URL`, the ignored test is skipped; default `cargo test` still
//! checks that the SQLite `connect` path rejects postgres URLs.

#![cfg(feature = "postgres")]

use atlas_inventory::{connect, connect_postgres, migrate_postgres};

#[tokio::test]
async fn sqlite_connect_still_rejects_postgres_url() {
    let err = connect("postgres://atlas:atlas@127.0.0.1:5432/atlas")
        .await
        .unwrap_err()
        .to_string();
    assert!(err.contains("docs/HA.md"), "{err}");
    assert!(
        err.contains("postgres") || err.contains("PostgreSQL"),
        "{err}"
    );
}

#[tokio::test]
#[ignore = "requires live Postgres; set DATABASE_URL and run with --ignored"]
async fn connect_and_migrate_postgres() {
    let url = std::env::var("DATABASE_URL")
        .or_else(|_| std::env::var("ATLAS_DATABASE_URL"))
        .expect("DATABASE_URL or ATLAS_DATABASE_URL must be set for this ignored test");
    let pool = connect_postgres(&url)
        .await
        .unwrap_or_else(|e| panic!("connect_postgres failed: {e}"));
    migrate_postgres(&pool)
        .await
        .unwrap_or_else(|e| panic!("migrate_postgres failed: {e}"));
    let n: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM storage_backends")
        .fetch_one(&pool)
        .await
        .expect("probe query after migrate");
    assert!(n >= 0);
}
