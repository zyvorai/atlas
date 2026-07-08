// Copyright (c) 2026 ZyvorAI Labs Private Limited. All rights reserved.
//! Environment-driven configuration with secret redaction (mirrors the ragnarok/machina pattern).

use std::fmt;

const DEV_JWT_DEFAULT: &str = "atlas-dev-jwt-secret-change-me-please-32b";

/// Which Ceph driver implementation the gateway wires up.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CephDriverMode {
    /// Shell out to `ceph`/`rbd` against a real cluster.
    Real,
    /// Serve canned fixtures (local dev / tests without a Ceph cluster).
    Fake,
}

impl CephDriverMode {
    fn from_env_str(s: &str) -> Self {
        match s.trim().to_lowercase().as_str() {
            "real" => CephDriverMode::Real,
            _ => CephDriverMode::Fake,
        }
    }
}

#[derive(Clone)]
pub struct Config {
    pub bind_addr: String,
    /// gRPC listen address (empty disables the gRPC edge).
    pub grpc_addr: String,
    pub database_url: String,
    pub ceph_driver_mode: CephDriverMode,
    /// Optional explicit kubeconfig path for the live k8s driver (empty = default resolution).
    pub kubeconfig_path: Option<String>,
    pub jwt_secret: String,
    /// When true, protected REST routes AND the gRPC edge require a valid JWT; false (dev) = open.
    pub auth_required: bool,
    /// Monitor loop interval in seconds (0 disables the monitor/alerts worker).
    pub monitor_interval_secs: u64,
    /// Ceph mgr Prometheus `/metrics` URL to scrape (None disables metric collection).
    pub ceph_prometheus_url: Option<String>,
    /// Default backups to retain per volume (0 = unlimited); overridable per request.
    pub backup_keep: i64,
    /// Default max backup age in seconds (0 = no age limit); overridable per request.
    pub backup_max_age_secs: i64,
    /// Public RGW endpoint (`http://host:port`) used to sign presigned download URLs so they are
    /// reachable off-cluster. When None, the bucket's in-cluster endpoint is used.
    pub rgw_public_endpoint: Option<String>,
    /// How often (seconds) the protection-schedule worker checks for due snapshot schedules
    /// (0 disables it).
    pub snapshot_tick_secs: u64,
    /// Optional HTTPS listen address (empty disables TLS). Served alongside the plain HTTP listener.
    pub https_addr: Option<String>,
    /// PEM cert/key paths for HTTPS (both required unless `tls_self_signed`).
    pub tls_cert_path: Option<String>,
    pub tls_key_path: Option<String>,
    /// Generate a self-signed cert at startup when no cert/key is provided (lab convenience).
    pub tls_self_signed: bool,
}

impl Config {
    /// Load from environment. Calls `dotenvy::dotenv()` first so a local `.env` is honored.
    pub fn from_env() -> Self {
        let _ = dotenvy::dotenv();
        let ceph_driver_mode = CephDriverMode::from_env_str(
            &std::env::var("ATLAS_CEPH_DRIVER_MODE").unwrap_or_else(|_| "fake".into()),
        );
        let kubeconfig_path = std::env::var("ATLAS_KUBECONFIG")
            .ok()
            .filter(|s| !s.trim().is_empty());
        let auth_required = matches!(
            std::env::var("ATLAS_AUTH_REQUIRED")
                .unwrap_or_default()
                .as_str(),
            "1" | "true" | "yes"
        );
        Self {
            bind_addr: std::env::var("ATLAS_BIND_ADDR").unwrap_or_else(|_| "127.0.0.1:5110".into()),
            grpc_addr: std::env::var("ATLAS_GRPC_ADDR").unwrap_or_else(|_| "127.0.0.1:5111".into()),
            database_url: std::env::var("ATLAS_DATABASE_URL")
                .unwrap_or_else(|_| "sqlite://atlas.db?mode=rwc".into()),
            ceph_driver_mode,
            kubeconfig_path,
            jwt_secret: std::env::var("ATLAS_JWT_SECRET")
                .unwrap_or_else(|_| DEV_JWT_DEFAULT.into()),
            auth_required,
            monitor_interval_secs: std::env::var("ATLAS_MONITOR_INTERVAL_SECS")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(60),
            ceph_prometheus_url: std::env::var("ATLAS_CEPH_PROMETHEUS_URL")
                .ok()
                .filter(|s| !s.trim().is_empty()),
            backup_keep: std::env::var("ATLAS_BACKUP_KEEP")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(0),
            backup_max_age_secs: std::env::var("ATLAS_BACKUP_MAX_AGE_SECS")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(0),
            rgw_public_endpoint: std::env::var("ATLAS_RGW_PUBLIC_ENDPOINT")
                .ok()
                .filter(|s| !s.trim().is_empty()),
            snapshot_tick_secs: std::env::var("ATLAS_SNAPSHOT_TICK_SECS")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(60),
            https_addr: std::env::var("ATLAS_HTTPS_ADDR")
                .ok()
                .filter(|s| !s.trim().is_empty()),
            tls_cert_path: std::env::var("ATLAS_TLS_CERT")
                .ok()
                .filter(|s| !s.trim().is_empty()),
            tls_key_path: std::env::var("ATLAS_TLS_KEY")
                .ok()
                .filter(|s| !s.trim().is_empty()),
            tls_self_signed: matches!(
                std::env::var("ATLAS_TLS_SELF_SIGNED")
                    .unwrap_or_default()
                    .as_str(),
                "1" | "true" | "yes"
            ),
        }
    }

    /// True when the JWT secret is the shipped dev default or too short to be safe.
    pub fn jwt_secret_is_weak(&self) -> bool {
        self.jwt_secret == DEV_JWT_DEFAULT || self.jwt_secret.len() < 32
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            bind_addr: "127.0.0.1:5110".into(),
            grpc_addr: "127.0.0.1:5111".into(),
            database_url: "sqlite://atlas.db?mode=rwc".into(),
            ceph_driver_mode: CephDriverMode::Fake,
            kubeconfig_path: None,
            jwt_secret: DEV_JWT_DEFAULT.into(),
            auth_required: false,
            monitor_interval_secs: 0,
            ceph_prometheus_url: None,
            backup_keep: 0,
            backup_max_age_secs: 0,
            rgw_public_endpoint: None,
            snapshot_tick_secs: 0,
            https_addr: None,
            tls_cert_path: None,
            tls_key_path: None,
            tls_self_signed: false,
        }
    }
}

/// Redact secrets in debug output so config is safe to log.
impl fmt::Debug for Config {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Config")
            .field("bind_addr", &self.bind_addr)
            .field("database_url", &self.database_url)
            .field("ceph_driver_mode", &self.ceph_driver_mode)
            .field("kubeconfig_path", &self.kubeconfig_path)
            .field("jwt_secret", &"<redacted>")
            .field("auth_required", &self.auth_required)
            .finish()
    }
}
