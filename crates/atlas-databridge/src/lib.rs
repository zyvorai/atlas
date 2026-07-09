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
pub mod pipeline;
pub mod reconcile;

pub use connector::{DiscoveredSchema, SourceCloud, SourceConnector, SourceKind, TableInfo};

/// Build a source connector for a registered source. In `fake` driver mode (default) this returns
/// the canned `FakeSourceConnector` so the pipeline runs with no cloud credentials; `real` mode
/// (added in a later slice) builds a live PostgreSQL/MySQL connector from the source's Secret.
pub fn build_connector(source: &MigrationSource) -> anyhow::Result<Box<dyn SourceConnector>> {
    let kind = SourceKind::parse(&source.kind)
        .ok_or_else(|| anyhow::anyhow!("unsupported source kind: {}", source.kind))?;
    match source.driver_mode.as_str() {
        "fake" => Ok(Box::new(connectors::fake::FakeSourceConnector::new(
            source.id.clone(),
            kind,
        ))),
        "real" => anyhow::bail!("real source connectors are not implemented in this slice"),
        other => anyhow::bail!("unknown driver_mode: {other}"),
    }
}
