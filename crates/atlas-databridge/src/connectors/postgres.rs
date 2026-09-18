// Copyright (c) 2026 ZyvorAI Labs Private Limited.
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-1.0
//! Live PostgreSQL source connector (RDS/Aurora/Cloud SQL or any PG endpoint). Introspects the
//! schema over a real connection so the pipeline can assess readiness for a real migration.
//!
//! TLS: this MVP connects without TLS (`sslmode=disable`). Cloud endpoints that require SSL need a
//! TLS connector (postgres-native-tls / rustls) — a follow-up.

use anyhow::{Context, Result};
use async_trait::async_trait;
use tokio_postgres::NoTls;

use crate::connector::{DiscoveredSchema, SourceConnector, TableInfo};

pub struct PostgresSourceConnector {
    source_id: String,
    conn_str: String,
}

impl PostgresSourceConnector {
    pub fn new(
        source_id: impl Into<String>,
        host: &str,
        port: i64,
        database: &str,
        user: &str,
        password: &str,
    ) -> Self {
        // sslmode=disable for the MVP; cloud SSL is a follow-up.
        let conn_str = format!(
            "host={host} port={port} dbname={database} user={user} password={password} sslmode=disable connect_timeout=10"
        );
        Self {
            source_id: source_id.into(),
            conn_str,
        }
    }

    async fn introspect(&self, client: &tokio_postgres::Client) -> Result<DiscoveredSchema> {
        let version: String = client
            .query_one("SELECT current_setting('server_version')", &[])
            .await
            .context("read server_version")?
            .get(0);
        let wal_level: String = client
            .query_one("SELECT current_setting('wal_level')", &[])
            .await
            .map(|r| r.get(0))
            .unwrap_or_else(|_| "unknown".to_string());
        let current_db: String = client
            .query_one("SELECT current_database()", &[])
            .await?
            .get(0);

        let databases: Vec<String> = client
            .query(
                "SELECT datname FROM pg_database WHERE datistemplate = false ORDER BY 1",
                &[],
            )
            .await?
            .iter()
            .map(|r| r.get::<_, String>(0))
            .collect();

        let extensions: Vec<String> = client
            .query("SELECT extname FROM pg_extension ORDER BY 1", &[])
            .await?
            .iter()
            .map(|r| r.get::<_, String>(0))
            .collect();

        let table_rows = client
            .query(
                "SELECT n.nspname AS schema, c.relname AS name, \
                        c.reltuples::bigint AS est_rows, \
                        pg_total_relation_size(c.oid) AS size_bytes, \
                        EXISTS(SELECT 1 FROM pg_index i WHERE i.indrelid = c.oid AND i.indisprimary) AS has_pk \
                 FROM pg_class c JOIN pg_namespace n ON n.oid = c.relnamespace \
                 WHERE c.relkind = 'r' AND n.nspname NOT IN ('pg_catalog','information_schema') \
                 ORDER BY 1,2",
                &[],
            )
            .await
            .context("introspect tables")?;
        let tables: Vec<TableInfo> = table_rows
            .iter()
            .map(|r| TableInfo {
                schema: r.get("schema"),
                name: r.get("name"),
                est_rows: r.get::<_, i64>("est_rows").max(0),
                size_bytes: r.get::<_, i64>("size_bytes").max(0),
                has_primary_key: r.get("has_pk"),
            })
            .collect();
        let total_size_bytes = tables.iter().map(|t| t.size_bytes).sum();

        Ok(DiscoveredSchema {
            engine: "postgres".to_string(),
            version,
            databases: if databases.is_empty() {
                vec![current_db]
            } else {
                databases
            },
            tables,
            extensions,
            total_size_bytes,
            cdc_capable: wal_level.eq_ignore_ascii_case("logical"),
        })
    }
}

#[async_trait]
impl SourceConnector for PostgresSourceConnector {
    fn source_id(&self) -> &str {
        &self.source_id
    }

    async fn discover(&self) -> Result<DiscoveredSchema> {
        let (client, connection) = tokio_postgres::connect(&self.conn_str, NoTls)
            .await
            .context("connect to source Postgres")?;
        // The connection future drives the protocol; run it until we're done introspecting.
        let handle = tokio::spawn(async move {
            let _ = connection.await;
        });
        let result = self.introspect(&client).await;
        drop(client);
        handle.abort();
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::connector::SourceConnector;

    /// Integration test against a real Postgres. Set DATABRIDGE_TEST_PG to a libpq-style host/db
    /// (e.g. "host=127.0.0.1 port=5599 dbname=appdb user=postgres password=x"); skipped otherwise.
    #[tokio::test]
    async fn discovers_real_postgres() {
        let Ok(spec) = std::env::var("DATABRIDGE_TEST_PG") else {
            eprintln!("skipping: set DATABRIDGE_TEST_PG to run");
            return;
        };
        // spec is a full conn string; wrap it directly.
        let conn = PgTestConn {
            source_id: "src_test".into(),
            conn_str: spec,
        };
        let schema = conn.discover().await.expect("discover");
        assert_eq!(schema.engine, "postgres");
        assert!(schema.tables.iter().any(|t| t.name == "customers"));
        let orders = schema
            .tables
            .iter()
            .find(|t| t.name == "orders")
            .expect("orders table");
        assert!(orders.has_primary_key);
    }

    // Test shim that lets the integration test pass a full conn string.
    struct PgTestConn {
        source_id: String,
        conn_str: String,
    }
    #[async_trait]
    impl SourceConnector for PgTestConn {
        fn source_id(&self) -> &str {
            &self.source_id
        }
        async fn discover(&self) -> anyhow::Result<crate::connector::DiscoveredSchema> {
            let real = PostgresSourceConnector {
                source_id: self.source_id.clone(),
                conn_str: self.conn_str.clone(),
            };
            real.discover().await
        }
    }
}
