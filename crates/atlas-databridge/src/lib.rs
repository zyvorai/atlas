// Copyright (c) 2026 ZyvorAI Labs Private Limited. All rights reserved.
//! Zyvor DataBridge — cloud-to-edge database migration control plane.
//!
//! Layered on Atlas: connectors introspect cloud source databases (PostgreSQL/MySQL on AWS RDS/Aurora
//! or GCP Cloud SQL), assessment scores readiness, and the pipeline provisions an edge database
//! (CloudNativePG / MySQL operator) on Ceph-backed storage, runs full-load + Debezium CDC, validates,
//! and cuts over. This crate isolates the connector/CR-templating logic (mirrors the driver crates).

use atlas_api_types::MigrationSource;

pub mod assess;
pub mod connector;
pub mod connectors;
pub mod cr;
pub mod loader;
pub mod pipeline;
pub mod reconcile;
pub mod validate;

pub use connector::{DiscoveredSchema, SourceCloud, SourceConnector, SourceKind, TableInfo};

/// Build a source connector for a registered source. In `fake` driver mode this returns the canned
/// `FakeSourceConnector` (no credentials needed); `real` mode builds a live PostgreSQL connector from
/// the source endpoint + `creds` (username, password) resolved from the source's k8s Secret.
pub fn build_connector(
    source: &MigrationSource,
    creds: Option<(&str, &str)>,
) -> anyhow::Result<Box<dyn SourceConnector>> {
    let kind = SourceKind::parse(&source.kind)
        .ok_or_else(|| anyhow::anyhow!("unsupported source kind: {}", source.kind))?;
    match source.driver_mode.as_str() {
        "fake" => Ok(Box::new(connectors::fake::FakeSourceConnector::new(
            source.id.clone(),
            kind,
        ))),
        "real" => {
            let (user, password) =
                creds.ok_or_else(|| anyhow::anyhow!("real connector requires credentials"))?;
            match kind {
                SourceKind::Postgres => Ok(Box::new(
                    connectors::postgres::PostgresSourceConnector::new(
                        source.id.clone(),
                        source.endpoint.as_deref().unwrap_or(""),
                        source.port.unwrap_or(5432),
                        source.database.as_deref().unwrap_or("appdb"),
                        user,
                        password,
                    ),
                )),
                SourceKind::Mysql => {
                    anyhow::bail!("real MySQL source connector is not implemented yet")
                }
            }
        }
        other => anyhow::bail!("unknown driver_mode: {other}"),
    }
}
