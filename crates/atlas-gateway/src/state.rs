// Copyright (c) 2026 ZyvorAI Labs Private Limited.
// SPDX-License-Identifier: AGPL-3.0-only OR LicenseRef-Atlas-Commercial
//! Shared application state injected into every axum handler.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use atlas_common::Config;
use atlas_driver_core::DriverRegistry;
use atlas_driver_k8s::K8sDriver;
use openidconnect::{EndpointMaybeSet, EndpointNotSet, EndpointSet};
use sqlx::SqlitePool;

/// `CoreClient::from_provider_metadata(..).set_redirect_uri(..)`'s exact concrete type — the
/// crate's endpoint-presence type-state machinery means this can't be named as the bare
/// `openidconnect::core::CoreClient` alias (that alias defaults every endpoint to
/// `EndpointNotSet`, but discovery always sets the auth/token endpoints and optionally the
/// userinfo endpoint). Determined empirically via a deliberate type-mismatch compile probe
/// against openidconnect 4.0.1 / oauth2 5.0.0 — if either crate is upgraded and this stops
/// compiling, re-derive it the same way rather than guessing.
type DiscoveredOidcClient = openidconnect::core::CoreClient<
    EndpointSet,      // HasAuthUrl — discovery always provides authorization_endpoint
    EndpointNotSet,   // HasDeviceAuthUrl — not part of OIDC discovery
    EndpointNotSet,   // HasIntrospectionUrl — not part of OIDC discovery
    EndpointNotSet,   // HasRevocationUrl — not set unless discovered separately (we don't)
    EndpointMaybeSet, // HasTokenUrl — discovery *should* provide token_endpoint but it's optional
    EndpointMaybeSet, // HasUserInfoUrl — userinfo_endpoint is optional in the discovery doc
>;

/// OIDC login state held between the `/auth/oidc/login` redirect and the `/auth/oidc/callback`
/// return trip, keyed by the CSRF `state` parameter. In-memory only (single-gateway-instance
/// deployment, per the rest of this codebase's HA posture) — a login abandoned mid-flow just
/// leaks one small entry until `sweep_expired` clears it.
pub(crate) struct PendingOidcLogin {
    pub nonce: String,
    pub pkce_verifier: String,
    pub created_at: Instant,
}

/// Runtime OIDC client, built once at startup by discovering the configured issuer. Wraps the
/// discovered `CoreClient` plus the in-flight-login map described above. `AppState.oidc` is
/// `None` whenever `Config.oidc` is `None` (feature not configured) or discovery failed at boot
/// (logged as a warning — the gateway still starts, just without SSO).
pub struct OidcRuntime {
    pub(crate) client: DiscoveredOidcClient,
    pub(crate) http: openidconnect::reqwest::Client,
    pending: Mutex<HashMap<String, PendingOidcLogin>>,
}

const PENDING_LOGIN_TTL_SECS: u64 = 600;

impl OidcRuntime {
    pub(crate) fn new(client: DiscoveredOidcClient, http: openidconnect::reqwest::Client) -> Self {
        Self { client, http, pending: Mutex::new(HashMap::new()) }
    }

    pub(crate) fn stash(&self, csrf_state: String, nonce: String, pkce_verifier: String) {
        let mut g = match self.pending.lock() {
            Ok(g) => g,
            Err(_) => return,
        };
        g.retain(|_, v| v.created_at.elapsed().as_secs() < PENDING_LOGIN_TTL_SECS);
        g.insert(csrf_state, PendingOidcLogin { nonce, pkce_verifier, created_at: Instant::now() });
    }

    /// Consume (remove) the pending login for `csrf_state`, if present and not expired.
    pub(crate) fn take(&self, csrf_state: &str) -> Option<PendingOidcLogin> {
        let mut g = self.pending.lock().ok()?;
        let entry = g.remove(csrf_state)?;
        if entry.created_at.elapsed().as_secs() >= PENDING_LOGIN_TTL_SECS {
            return None;
        }
        Some(entry)
    }
}

/// Discover the configured OIDC issuer and build a runtime client. Returns `None` (logged as a
/// warning, never fatal) on any failure — a lab Dex instance being briefly unreachable at boot
/// shouldn't take down the rest of the gateway; the "Sign in with SSO" button just won't appear.
pub async fn build_oidc_runtime(cfg: &Config) -> Option<Arc<OidcRuntime>> {
    let oidc_cfg = cfg.oidc.as_ref()?;
    use openidconnect::core::{CoreClient, CoreProviderMetadata};
    use openidconnect::{ClientId, ClientSecret, IssuerUrl, RedirectUrl};

    let issuer_url = match IssuerUrl::new(oidc_cfg.issuer_url.clone()) {
        Ok(u) => u,
        Err(e) => {
            tracing::warn!("OIDC issuer URL invalid, SSO disabled: {e}");
            return None;
        }
    };
    let redirect_url = match RedirectUrl::new(oidc_cfg.redirect_url.clone()) {
        Ok(u) => u,
        Err(e) => {
            tracing::warn!("OIDC redirect URL invalid, SSO disabled: {e}");
            return None;
        }
    };
    // A throwaway/lab IdP is commonly plain HTTP with no reverse proxy in front of it — this
    // client is used server-to-server only (discovery + token exchange), never to fetch
    // anything an end user supplies, so following redirects isn't the SSRF risk it would be for
    // a general-purpose HTTP client. `reqwest::Client` (async) implements openidconnect's
    // `AsyncHttpClient` directly, so no adapter type is needed.
    let http = openidconnect::reqwest::Client::builder()
        .redirect(openidconnect::reqwest::redirect::Policy::none())
        .build()
        .ok()?;

    let metadata = match CoreProviderMetadata::discover_async(issuer_url, &http).await {
        Ok(m) => m,
        Err(e) => {
            tracing::warn!("OIDC discovery against {} failed, SSO disabled: {e}", oidc_cfg.issuer_url);
            return None;
        }
    };

    let client = CoreClient::from_provider_metadata(
        metadata,
        ClientId::new(oidc_cfg.client_id.clone()),
        Some(ClientSecret::new(oidc_cfg.client_secret.clone())),
    )
    .set_redirect_uri(redirect_url);

    tracing::info!(issuer = %oidc_cfg.issuer_url, "OIDC/SSO login enabled");
    Some(Arc::new(OidcRuntime::new(client, http)))
}

/// In-process worker heartbeats: each periodic worker stamps its name every tick so `/readyz` can
/// surface a wedged worker. Empty when workers are disabled (e.g. tests), which readyz treats as
/// "nothing to check" rather than a failure.
#[derive(Clone, Default)]
pub struct WorkerHealth {
    beats: Arc<Mutex<HashMap<&'static str, Instant>>>,
}

impl WorkerHealth {
    /// Record a heartbeat for `worker` (called at the top of each worker loop iteration).
    pub fn beat(&self, worker: &'static str) {
        if let Ok(mut g) = self.beats.lock() {
            g.insert(worker, Instant::now());
        }
    }

    /// `(worker, seconds_since_last_beat)` for every worker that has beat at least once.
    pub fn ages(&self) -> Vec<(&'static str, u64)> {
        self.beats
            .lock()
            .map(|g| g.iter().map(|(k, v)| (*k, v.elapsed().as_secs())).collect())
            .unwrap_or_default()
    }
}

/// Per-actor fixed-window rate limiter (day-2 governance). `rpm == 0` disables it. Keyed by actor id;
/// each 60s window resets the count. In-process (single-replica); a distributed limiter is a later
/// slice. Configured from `ATLAS_RATE_LIMIT_RPM` at startup.
#[derive(Clone)]
pub struct RateLimiter {
    rpm: u32,
    windows: Arc<Mutex<HashMap<String, (u64, u32)>>>,
}

impl RateLimiter {
    pub fn new(rpm: u32) -> Self {
        Self { rpm, windows: Arc::new(Mutex::new(HashMap::new())) }
    }

    /// Whether the request is allowed. Increments the actor's count for the current minute and
    /// returns false once it exceeds `rpm`. Always true when disabled (`rpm == 0`).
    pub fn allow(&self, actor: &str) -> bool {
        if self.rpm == 0 {
            return true;
        }
        let minute = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() / 60)
            .unwrap_or(0);
        let mut g = match self.windows.lock() {
            Ok(g) => g,
            Err(_) => return true, // never lock out on a poisoned mutex
        };
        let entry = g.entry(actor.to_string()).or_insert((minute, 0));
        if entry.0 != minute {
            *entry = (minute, 0);
        }
        entry.1 += 1;
        entry.1 <= self.rpm
    }
}

#[derive(Clone)]
pub struct AppState {
    pub pool: SqlitePool,
    pub config: Arc<Config>,
    /// Backend storage drivers keyed by backend id (Ceph real/fake in the MVP).
    pub drivers: Arc<DriverRegistry>,
    /// Live Kubernetes driver, if a cluster was reachable at startup.
    pub k8s: Option<Arc<K8sDriver>>,
    /// Async job engine (write path).
    pub jobs: atlas_jobs::JobEngine,
    /// Periodic-worker heartbeats for `/readyz`.
    pub workers: WorkerHealth,
    /// Per-actor request rate limiter (day-2 governance).
    pub rate: RateLimiter,
    /// OIDC/SSO login, when configured and discovery succeeded at startup.
    pub oidc: Option<Arc<OidcRuntime>>,
}

impl AppState {
    /// Resolve the driver for a backend id, or the only one when there's a single backend.
    pub fn driver_for(
        &self,
        backend_id: &str,
    ) -> Option<Arc<dyn atlas_driver_core::StorageDriver>> {
        self.drivers.get(backend_id).or_else(|| self.drivers.any())
    }

    /// Build the RBD-image → owning-PVC map from cluster PersistentVolumes, so discovery can
    /// attribute raw `rbd ls` images to their Kubernetes VM disk. `None` when no cluster is
    /// attached or the PV list fails (discovery then just leaves those volumes unattributed).
    pub async fn rbd_owners(&self) -> Option<atlas_discovery::RbdOwners> {
        let k8s = self.k8s.as_ref()?;
        match k8s.rbd_image_owners().await {
            Ok(m) => Some(
                m.into_iter()
                    .map(|(img, o)| (img, (o.namespace, o.pvc_name, o.storage_class)))
                    .collect(),
            ),
            Err(e) => {
                tracing::warn!("rbd_image_owners failed: {e}");
                None
            }
        }
    }

    /// Build the pool-name → precise-kind map from live Rook CRs (`config.rook_namespace`), so
    /// discovery can classify pools exactly instead of guessing from the name. `None` when no
    /// cluster is attached or the CR list fails (discovery then just keeps the name heuristic).
    pub async fn rook_pool_kinds(&self) -> Option<atlas_discovery::RookPoolKinds> {
        let k8s = self.k8s.as_ref()?;
        match k8s.known_rook_pool_kinds(&self.config.rook_namespace).await {
            Ok(m) => Some(
                m.into_iter()
                    .map(|(pool, kind)| (pool, kind.as_str().to_string()))
                    .collect(),
            ),
            Err(e) => {
                tracing::warn!("known_rook_pool_kinds failed: {e}");
                None
            }
        }
    }
}
