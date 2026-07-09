// Copyright (c) 2026 ZyvorAI Labs Private Limited. All rights reserved.
//! The source-connector contract: discover a cloud/source database's schema so the pipeline can
//! assess readiness, provision an edge target, and plan the load. Mirrors the `StorageDriver`
//! trait split — the gateway/pipeline depend only on this trait, never on tokio-postgres/mysql.

use anyhow::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

/// Source database engine.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SourceKind {
    Postgres,
    Mysql,
}

impl SourceKind {
    pub fn parse(s: &str) -> Option<Self> {
        match s.to_ascii_lowercase().as_str() {
            "postgres" | "postgresql" | "pg" => Some(Self::Postgres),
            "mysql" | "mariadb" => Some(Self::Mysql),
            _ => None,
        }
    }
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Postgres => "postgres",
            Self::Mysql => "mysql",
        }
    }
}

/// Which managed-cloud flavor the source is (affects TLS/auth/privilege preflight only).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SourceCloud {
    Rds,
    Aurora,
    CloudSql,
    Generic,
}

impl SourceCloud {
    pub fn parse(s: &str) -> Self {
        match s.to_ascii_lowercase().as_str() {
            "rds" => Self::Rds,
            "aurora" => Self::Aurora,
            "cloudsql" | "cloud-sql" => Self::CloudSql,
            _ => Self::Generic,
        }
    }
}

/// One table discovered on the source, with the facts assessment needs.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TableInfo {
    pub schema: String,
    pub name: String,
    pub est_rows: i64,
    pub size_bytes: i64,
    /// CDC needs a primary key (or replica identity) — a missing PK is an assessment blocker.
    pub has_primary_key: bool,
}

/// The full schema snapshot returned by a discovery pass. Serialized into `migration_sources.discovered`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscoveredSchema {
    pub engine: String,
    pub version: String,
    pub databases: Vec<String>,
    pub tables: Vec<TableInfo>,
    /// Extensions (postgres) / plugins (mysql) present on the source.
    pub extensions: Vec<String>,
    pub total_size_bytes: i64,
    /// Whether the server has logical decoding / binlog enabled for CDC.
    pub cdc_capable: bool,
}

/// A backend that can introspect one source database.
#[async_trait]
pub trait SourceConnector: Send + Sync {
    /// The registered source id this connector serves.
    fn source_id(&self) -> &str;
    /// Full discovery pass: engine/version + databases + tables + sizes + CDC capability.
    async fn discover(&self) -> Result<DiscoveredSchema>;
}
