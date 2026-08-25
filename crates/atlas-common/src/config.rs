// Copyright (c) 2026 ZyvorAI Labs Private Limited. All rights reserved.
//! Environment-driven configuration with secret redaction (mirrors the ragnarok/machina pattern).

use std::fmt;

const DEV_JWT_DEFAULT: &str = "atlas-dev-jwt-secret-change-me-please-32b";
/// Dev-only fallback console admin password — fine for local `make run` (auth disabled), but
/// `validate_for_start()` refuses to boot with this value when `ATLAS_AUTH_REQUIRED` is set. Real
/// deployments get a strong random one from `scripts/ensure-atlas-auth-secret.sh` via
/// `secretKeyRef` (see `deploy/k8s/atlas-gateway*.yaml`), never this literal.
const DEV_ADMIN_PASSWORD_DEFAULT: &str = "Admin@321";

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

/// OIDC/SSO login configuration. Only built when `ATLAS_OIDC_ISSUER_URL`, `_CLIENT_ID`, and
/// `_REDIRECT_URL` are all set — absence disables the feature entirely (no route exposed, no
/// "Sign in with SSO" button), mirroring how `ceph_prometheus_url`/`alert_webhook_url` auto-gate.
/// A successful OIDC login mints the same Atlas-issued HS256 JWT `POST /auth/login` does — OIDC is
/// only ever a second way to *obtain* a token, never a parallel validation path, so
/// `auth_middleware`/`require_role`/token revocation are all untouched.
#[derive(Clone)]
pub struct OidcConfig {
    pub issuer_url: String,
    pub client_id: String,
    pub client_secret: String,
    pub redirect_url: String,
    /// OIDC group names (from the `groups` claim) that map to the `admin` role.
    pub admin_groups: Vec<String>,
    /// OIDC group names that map to the `operator` role. Anything matching neither list, or a
    /// token with no `groups` claim at all, gets `viewer` (least privilege by default).
    pub operator_groups: Vec<String>,
    /// Name of the ID-token claim carrying the caller's tenant id (e.g. `"tenant"` or
    /// `"department"` — configurable since enterprise IdPs vary in what they call it). A token
    /// missing this claim, or an unset `ATLAS_OIDC_TENANT_CLAIM`, resolves to the `"global"`
    /// tenant so existing single-tenant deployments keep working unchanged.
    pub tenant_claim: Option<String>,
}

impl fmt::Debug for OidcConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("OidcConfig")
            .field("issuer_url", &self.issuer_url)
            .field("client_id", &self.client_id)
            .field("client_secret", &"<redacted>")
            .field("redirect_url", &self.redirect_url)
            .field("admin_groups", &self.admin_groups)
            .field("operator_groups", &self.operator_groups)
            .field("tenant_claim", &self.tenant_claim)
            .finish()
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
    /// Previous JWT secret, still accepted for *validating* tokens (never for minting new ones)
    /// during a rotation window — set this to the outgoing `ATLAS_JWT_SECRET` value when rotating
    /// to a new one, so already-issued tokens keep working until they naturally expire instead of
    /// every session being force-logged-out the moment the secret changes. Remove once confident
    /// no outstanding token still uses it (bounded by each token's own TTL, capped at 90 days).
    pub jwt_secret_previous: Option<String>,
    /// When true, protected REST routes AND the gRPC edge require a valid JWT; false (dev) = open.
    pub auth_required: bool,
    /// Optional one-shot bootstrap admin bearer (raw string, not a JWT). Accepted only when
    /// `auth_required` so a fresh deploy can mint real service-account tokens, then unset this.
    pub bootstrap_admin_token: Option<String>,
    /// Console operator username for `POST /auth/login` (default `admin`).
    pub admin_username: String,
    /// Console operator password for `POST /auth/login` (default `Admin@321`). Override in prod.
    pub admin_password: String,
    /// Monitor loop interval in seconds (0 disables the monitor/alerts worker).
    pub monitor_interval_secs: u64,
    /// Ceph mgr Prometheus `/metrics` URL to scrape (None disables metric collection).
    pub ceph_prometheus_url: Option<String>,
    /// Optional webhook URL (Slack-compatible JSON) notified when an alert fires/resolves.
    pub alert_webhook_url: Option<String>,
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
    /// How often (seconds) the DataBridge reconciler advances migration pipelines / CDC lag
    /// (0 disables it).
    pub databridge_reconcile_secs: u64,
    /// How often (seconds) the job engine polls SQLite for due `queued`/`pending` work. The
    /// in-memory channel is a fast wake-up; the DB poller is the durable source of truth
    /// (survives channel loss, honors `next_attempt_at`). `0` disables the poller (tests).
    pub job_poll_secs: u64,
    /// Reclaim `running` jobs whose `locked_at`/`updated_at` is older than this many seconds
    /// (stale mid-flight after a hard kill that never ran boot recovery). `0` disables reclaim.
    pub job_stale_secs: u64,
    /// Optional HTTPS listen address (empty disables TLS). Served alongside the plain HTTP
    /// listener unless `disable_http` is also set.
    pub https_addr: Option<String>,
    /// PEM cert/key paths for HTTPS (both required unless `tls_self_signed`).
    pub tls_cert_path: Option<String>,
    pub tls_key_path: Option<String>,
    /// Generate a self-signed cert at startup when no cert/key is provided (lab convenience).
    pub tls_self_signed: bool,
    /// Skip binding the plain HTTP listener entirely, so TLS can't be bypassed by hitting the
    /// HTTP port directly. Only takes effect when HTTPS is actually configured —
    /// `validate_for_start()` refuses to boot with this set but no working HTTPS listener, since
    /// that would mean no REST listener at all.
    pub disable_http: bool,
    /// Register a second NFS backend (demonstrates the pluggable-driver architecture).
    pub nfs_enable: bool,
    /// NFS server host for the NFS backend (defaults to a demo host when enabled without one).
    pub nfs_server: Option<String>,
    /// Comma-separated NFS export paths (defaults to demo exports when enabled without any).
    pub nfs_exports: Vec<String>,
    /// Register a third ZFS backend (demonstrates the pluggable-driver architecture scaling).
    pub zfs_enable: bool,
    /// ZFS host for the ZFS backend (defaults to a demo host when enabled without one).
    pub zfs_host: Option<String>,
    /// Comma-separated zpool names (defaults to demo zpools when enabled without any).
    pub zfs_pools: Vec<String>,
    /// OIDC/SSO login (`None` = feature disabled — no unauthenticated OIDC routes are mounted).
    pub oidc: Option<OidcConfig>,
    /// Kubernetes namespace the Rook operator/CephCluster runs in — used when reading Rook's own
    /// `ceph.rook.io/v1` CRs (CephCluster/CephBlockPool/CephFilesystem/CephObjectStore) as a
    /// second, precise source of truth alongside the `ceph`/`rbd` CLI path.
    pub rook_namespace: String,
    /// Name of the `CephCluster` CR within `rook_namespace` (the lab always names it `rook-ceph`).
    pub rook_cluster_name: String,
    /// True only once an operator has personally run the live two-site `rbd mirror` drill
    /// documented in `docs/DR.md`'s "Live two-site checklist" against THIS deployment's real
    /// Ceph cluster and a real peer — never set generically. Mirrors `license_enforce`'s
    /// pattern: a hard-coded blanket claim here would be dishonest for any deployment that
    /// hasn't actually done the drill, so it's a per-deployment config toggle instead. Read by
    /// `GET /dr/status`'s `dataplane_verified` field; defaults to `false` (control-plane-only,
    /// matching every deployment until proven otherwise).
    pub dr_dataplane_verified: bool,
    /// When true (the default), an expired/missing/invalid trial or license token causes
    /// protected REST routes to return 402 (see `atlas-gateway::license`). Token *location*
    /// (env var, file) is resolved per-request, not cached here, so swapping a license file
    /// takes effect without a restart. Set `ATLAS_LICENSE_ENFORCE=false` for local dev/tests —
    /// mirrors Aurora's `AURORA_LICENSE_ENFORCE` default-true toggle.
    pub license_enforce: bool,
}

fn oidc_from_env() -> Option<OidcConfig> {
    let issuer_url = std::env::var("ATLAS_OIDC_ISSUER_URL")
        .ok()
        .filter(|s| !s.trim().is_empty())?;
    let client_id = std::env::var("ATLAS_OIDC_CLIENT_ID")
        .ok()
        .filter(|s| !s.trim().is_empty())?;
    let redirect_url = std::env::var("ATLAS_OIDC_REDIRECT_URL")
        .ok()
        .filter(|s| !s.trim().is_empty())?;
    let client_secret = std::env::var("ATLAS_OIDC_CLIENT_SECRET").unwrap_or_default();
    let split_csv = |var: &str| -> Vec<String> {
        std::env::var(var)
            .ok()
            .map(|s| {
                s.split(',')
                    .map(|x| x.trim().to_string())
                    .filter(|x| !x.is_empty())
                    .collect()
            })
            .unwrap_or_default()
    };
    Some(OidcConfig {
        issuer_url,
        client_id,
        client_secret,
        redirect_url,
        admin_groups: split_csv("ATLAS_OIDC_ADMIN_GROUP"),
        operator_groups: split_csv("ATLAS_OIDC_OPERATOR_GROUP"),
        tenant_claim: std::env::var("ATLAS_OIDC_TENANT_CLAIM")
            .ok()
            .filter(|s| !s.trim().is_empty()),
    })
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
            jwt_secret_previous: std::env::var("ATLAS_JWT_SECRET_PREVIOUS")
                .ok()
                .filter(|s| !s.trim().is_empty()),
            auth_required,
            bootstrap_admin_token: std::env::var("ATLAS_BOOTSTRAP_ADMIN_TOKEN")
                .ok()
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty()),
            admin_username: std::env::var("ATLAS_ADMIN_USERNAME")
                .ok()
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| "admin".into()),
            admin_password: std::env::var("ATLAS_ADMIN_PASSWORD")
                .ok()
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| DEV_ADMIN_PASSWORD_DEFAULT.into()),
            monitor_interval_secs: std::env::var("ATLAS_MONITOR_INTERVAL_SECS")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(60),
            ceph_prometheus_url: std::env::var("ATLAS_CEPH_PROMETHEUS_URL")
                .ok()
                .filter(|s| !s.trim().is_empty()),
            alert_webhook_url: std::env::var("ATLAS_ALERT_WEBHOOK_URL")
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
            databridge_reconcile_secs: std::env::var("ATLAS_DATABRIDGE_RECONCILE_SECS")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(15),
            job_poll_secs: std::env::var("ATLAS_JOB_POLL_SECS")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(2),
            job_stale_secs: std::env::var("ATLAS_JOB_STALE_SECS")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(900),
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
                    .trim()
                    .to_lowercase()
                    .as_str(),
                "1" | "true" | "yes"
            ),
            disable_http: matches!(
                std::env::var("ATLAS_DISABLE_HTTP")
                    .unwrap_or_default()
                    .trim()
                    .to_lowercase()
                    .as_str(),
                "1" | "true" | "yes"
            ),
            nfs_enable: matches!(
                std::env::var("ATLAS_NFS_ENABLE")
                    .unwrap_or_default()
                    .trim()
                    .to_lowercase()
                    .as_str(),
                "1" | "true" | "yes"
            ),
            nfs_server: std::env::var("ATLAS_NFS_SERVER")
                .ok()
                .filter(|s| !s.trim().is_empty()),
            nfs_exports: std::env::var("ATLAS_NFS_EXPORTS")
                .ok()
                .map(|s| {
                    s.split(',')
                        .map(|x| x.trim().to_string())
                        .filter(|x| !x.is_empty())
                        .collect()
                })
                .unwrap_or_default(),
            zfs_enable: matches!(
                std::env::var("ATLAS_ZFS_ENABLE")
                    .unwrap_or_default()
                    .trim()
                    .to_lowercase()
                    .as_str(),
                "1" | "true" | "yes"
            ),
            zfs_host: std::env::var("ATLAS_ZFS_HOST")
                .ok()
                .filter(|s| !s.trim().is_empty()),
            zfs_pools: std::env::var("ATLAS_ZFS_POOLS")
                .ok()
                .map(|s| {
                    s.split(',')
                        .map(|x| x.trim().to_string())
                        .filter(|x| !x.is_empty())
                        .collect()
                })
                .unwrap_or_default(),
            oidc: oidc_from_env(),
            rook_namespace: std::env::var("ATLAS_ROOK_NAMESPACE")
                .ok()
                .filter(|s| !s.trim().is_empty())
                .unwrap_or_else(|| "rook-ceph".into()),
            rook_cluster_name: std::env::var("ATLAS_ROOK_CLUSTER_NAME")
                .ok()
                .filter(|s| !s.trim().is_empty())
                .unwrap_or_else(|| "rook-ceph".into()),
            license_enforce: !matches!(
                std::env::var("ATLAS_LICENSE_ENFORCE")
                    .unwrap_or_default()
                    .trim()
                    .to_lowercase()
                    .as_str(),
                "0" | "false" | "no"
            ),
            dr_dataplane_verified: matches!(
                std::env::var("ATLAS_DR_DATAPLANE_VERIFIED")
                    .unwrap_or_default()
                    .trim()
                    .to_lowercase()
                    .as_str(),
                "1" | "true" | "yes"
            ),
        }
    }

    /// True when the JWT secret is the shipped dev default or too short to be safe.
    pub fn jwt_secret_is_weak(&self) -> bool {
        self.jwt_secret == DEV_JWT_DEFAULT || self.jwt_secret.len() < 32
    }

    /// True when the console admin password is the shipped dev default or too short to be safe.
    pub fn admin_password_is_weak(&self) -> bool {
        self.admin_password == DEV_ADMIN_PASSWORD_DEFAULT || self.admin_password.len() < 12
    }

    /// Refuse to start in authenticated mode with a forgeable secret or a known/weak admin
    /// password. Local `make run` keeps `ATLAS_AUTH_REQUIRED` unset/false so the weak defaults
    /// remain fine for lab use.
    pub fn validate_for_start(&self) -> Result<(), String> {
        if self.auth_required && self.jwt_secret_is_weak() {
            return Err(
                "ATLAS_AUTH_REQUIRED is set but ATLAS_JWT_SECRET is the shipped dev default or shorter than 32 bytes — refuse to start"
                    .into(),
            );
        }
        if self.auth_required && self.admin_password_is_weak() {
            return Err(
                "ATLAS_AUTH_REQUIRED is set but ATLAS_ADMIN_PASSWORD is the shipped dev default or shorter than 12 characters — refuse to start"
                    .into(),
            );
        }
        if self.disable_http
            && (self.https_addr.is_none() || self.tls_cert_path.is_none() || self.tls_key_path.is_none())
        {
            return Err(
                "ATLAS_DISABLE_HTTP is set but ATLAS_HTTPS_ADDR/ATLAS_TLS_CERT/ATLAS_TLS_KEY aren't all configured — that would leave no REST listener at all, refuse to start"
                    .into(),
            );
        }
        Ok(())
    }

    /// Non-fatal: warns (doesn't refuse to boot) when `license_enforce` is on but no
    /// trial/license token is locatable anywhere `atlas_license::locate_token_from_env` checks.
    /// A hard refuse-to-boot would be wrong here — an operator may legitimately deploy before
    /// receiving their token from sales — but booting silently into "every protected route
    /// 402s" with no signal at all is a real misconfiguration trap this at least surfaces
    /// loudly at startup instead of only being discovered via a customer's first support ticket.
    pub fn warn_if_license_misconfigured(&self) {
        if license_misconfigured(self.license_enforce, atlas_license::locate_token_from_env().is_some()) {
            tracing::warn!(
                "ATLAS_LICENSE_ENFORCE is on but no trial/license token was found via \
                 ATLAS_LICENSE_KEY, ATLAS_TRIAL_TOKEN, ATLAS_TRIAL_TOKEN_FILE, or ./trial.token — \
                 every protected REST route will return 402 until one is set. See docs/LICENSING.md."
            );
        }
    }
}

/// Pure decision extracted from [`Config::warn_if_license_misconfigured`] so it's unit-testable
/// without mutating process-global environment state (which `locate_token_from_env` reads).
fn license_misconfigured(license_enforce: bool, token_present: bool) -> bool {
    license_enforce && !token_present
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
            jwt_secret_previous: None,
            auth_required: false,
            bootstrap_admin_token: None,
            admin_username: "admin".into(),
            admin_password: DEV_ADMIN_PASSWORD_DEFAULT.into(),
            monitor_interval_secs: 0,
            ceph_prometheus_url: None,
            alert_webhook_url: None,
            backup_keep: 0,
            backup_max_age_secs: 0,
            rgw_public_endpoint: None,
            snapshot_tick_secs: 0,
            databridge_reconcile_secs: 0,
            job_poll_secs: 0,
            job_stale_secs: 0,
            https_addr: None,
            tls_cert_path: None,
            tls_key_path: None,
            tls_self_signed: false,
            disable_http: false,
            nfs_enable: false,
            nfs_server: None,
            nfs_exports: Vec::new(),
            zfs_enable: false,
            zfs_host: None,
            zfs_pools: Vec::new(),
            oidc: None,
            rook_namespace: "rook-ceph".into(),
            rook_cluster_name: "rook-ceph".into(),
            license_enforce: true,
            dr_dataplane_verified: false,
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
            .field(
                "jwt_secret_previous",
                &self.jwt_secret_previous.as_ref().map(|_| "<redacted>"),
            )
            .field("auth_required", &self.auth_required)
            .field(
                "bootstrap_admin_token",
                &self.bootstrap_admin_token.as_ref().map(|_| "<redacted>"),
            )
            .field("admin_username", &self.admin_username)
            .field("admin_password", &"<redacted>")
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_rejects_weak_secret_when_auth_required() {
        let mut c = Config {
            auth_required: true,
            ..Default::default()
        };
        assert!(c.validate_for_start().is_err());
        c.jwt_secret = "a-strong-enough-secret-at-least-32b!".into();
        // Still weak: admin_password is still the Default's dev fallback.
        assert!(c.validate_for_start().is_err());
        c.admin_password = "a-strong-enough-admin-password".into();
        assert!(c.validate_for_start().is_ok());
    }

    #[test]
    fn validate_rejects_weak_admin_password_when_auth_required() {
        let mut c = Config {
            auth_required: true,
            jwt_secret: "a-strong-enough-secret-at-least-32b!".into(),
            admin_password: "Admin@321".into(),
            ..Default::default()
        };
        assert!(c.validate_for_start().is_err());
        c.admin_password = "short".into();
        assert!(c.validate_for_start().is_err());
        c.admin_password = "a-strong-enough-admin-password".into();
        assert!(c.validate_for_start().is_ok());
    }

    #[test]
    fn validate_allows_weak_secret_when_auth_open() {
        let c = Config::default();
        assert!(!c.auth_required);
        assert!(c.validate_for_start().is_ok());
    }

    #[test]
    fn validate_rejects_disable_http_without_working_https() {
        let mut c = Config {
            jwt_secret: "a-strong-enough-secret-at-least-32b!".into(),
            admin_password: "a-strong-enough-admin-password".into(),
            disable_http: true,
            ..Default::default()
        };
        // No ATLAS_HTTPS_ADDR/TLS_CERT/TLS_KEY at all — would leave no REST listener.
        assert!(c.validate_for_start().is_err());
        c.https_addr = Some("0.0.0.0:5443".into());
        // Still missing cert/key.
        assert!(c.validate_for_start().is_err());
        c.tls_cert_path = Some("/etc/atlas-tls/tls.crt".into());
        c.tls_key_path = Some("/etc/atlas-tls/tls.key".into());
        assert!(c.validate_for_start().is_ok());
    }

    #[test]
    fn license_misconfigured_only_when_enforced_with_no_token() {
        assert!(license_misconfigured(true, false));
        assert!(!license_misconfigured(true, true));
        assert!(!license_misconfigured(false, false));
        assert!(!license_misconfigured(false, true));
    }
}
