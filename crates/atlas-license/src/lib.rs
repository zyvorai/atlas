// Copyright (c) 2026 ZyvorAI Labs Private Limited. All rights reserved.
// Proprietary software — see LICENSE in the repository root.
// https://zyvor.dev · info@zyvor.dev

//! Ed25519-signed trial/license tokens for Atlas — the same design already used by Veyron
//! (`veyron::trial`) and Aurora (`gtm_api.services.licensing`) across the Zyvor product
//! family. A token is a signed JWT (`LicenseClaims`) carrying who it was issued to and when it
//! expires; Atlas never runs its own clock for this — expiry lives inside the token, so
//! deleting local state cannot extend a trial. The private signing key is held only by Zyvor
//! sales (see the `atlas-license-tool` binary); this crate embeds only the public key.
//!
//! Token *lookup* (which env var, which file) is a server-side concern and lives in
//! `atlas-gateway::license`, not here — this crate only verifies a token it's handed and
//! reports status.

use base64::Engine;
use jsonwebtoken::{decode, Algorithm, DecodingKey, Validation};
use serde::{Deserialize, Serialize};

/// Ed25519 public key (raw 32 bytes, standard base64), matching the private key used by
/// `atlas-license-tool issue`. Regenerate with `cargo run -p atlas-license-tool -- keygen` and
/// replace this constant when rotating the signing key — doing so invalidates every previously
/// issued token, so reissue any trials still active.
pub const LICENSE_PUBLIC_KEY_B64: &str = "N6AwKuQ85ayORhxpC/MQO+EIQtzlJL8pzmZrfe345nI=";

pub const SALES_CONTACT: &str = "sales@zyvor.dev";

/// Marker embedded in every issued token so a token minted for a different Zyvor product can
/// never be mistaken for a valid Atlas license.
pub const PRODUCT_TAG: &str = "atlas-trial";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LicenseClaims {
    /// Who this was issued to (freeform — company/contact name).
    pub sub: String,
    pub iat: i64,
    pub exp: i64,
    pub product: String,
}

/// A verified token's status, ready to serialize as the `/license/status` response body.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LicenseStatus {
    /// A valid, non-expired token for this product is present.
    pub licensed: bool,
    pub trial_active: bool,
    pub trial_expired: bool,
    pub trial_days_remaining: i64,
    pub licensee: Option<String>,
    pub sales_contact: String,
}

fn decoding_key(pub_key_b64: &str) -> Result<DecodingKey, String> {
    let raw = base64::engine::general_purpose::STANDARD
        .decode(pub_key_b64)
        .map_err(|e| format!("public key is not valid base64: {e}"))?;
    Ok(DecodingKey::from_ed_der(&raw))
}

/// Core verification against an explicit public key — split out from [`verify`] so tests can
/// exercise it against a freshly generated test keypair without touching the embedded constant.
///
/// Deliberately does *not* set `validate_exp` — an expired-but-authentically-signed token must
/// still decode successfully here so [`status`] can report *why* it's inactive (`trial_expired:
/// true`, real `exp`) instead of it looking identical to no token being present at all. A token
/// that fails to decode at all (bad signature, wrong product, malformed) is the only case this
/// returns `Err` for. Callers that need "is this currently usable" must check
/// [`is_active`]/[`status`], never treat `Ok` from this function alone as sufficient.
fn verify_with_key(token: &str, pub_key_b64: &str) -> Result<LicenseClaims, String> {
    let key = decoding_key(pub_key_b64)?;
    let mut validation = Validation::new(Algorithm::EdDSA);
    validation.validate_exp = false;
    validation.validate_aud = false;
    let data = decode::<LicenseClaims>(token.trim(), &key, &validation)
        .map_err(|e| format!("invalid license token ({e})"))?;
    if data.claims.product != PRODUCT_TAG {
        return Err("token was not issued for this product".to_string());
    }
    Ok(data.claims)
}

/// Verifies signature and product tag against the embedded Zyvor public key — expiry is *not*
/// checked here, see [`verify_with_key`]'s doc comment for why. Never trusts an unsigned or
/// wrongly-tagged token.
pub fn verify(token: &str) -> Result<LicenseClaims, String> {
    verify_with_key(token, LICENSE_PUBLIC_KEY_B64)
}

fn status_from_claims(claims: Option<LicenseClaims>) -> LicenseStatus {
    let no_token = || LicenseStatus {
        licensed: false,
        trial_active: false,
        trial_expired: false,
        trial_days_remaining: 0,
        licensee: None,
        sales_contact: SALES_CONTACT.to_string(),
    };

    let Some(claims) = claims else {
        return no_token();
    };

    let now = chrono::Utc::now().timestamp();
    let remaining_secs = claims.exp - now;
    let expired = remaining_secs <= 0;
    let days_remaining = if expired {
        0
    } else {
        ((remaining_secs as f64) / 86_400.0).ceil() as i64
    };

    LicenseStatus {
        licensed: !expired,
        trial_active: !expired,
        trial_expired: expired,
        trial_days_remaining: days_remaining,
        licensee: Some(claims.sub),
        sales_contact: SALES_CONTACT.to_string(),
    }
}

/// Builds the `/license/status` response from an already-located raw token string (or `None` if
/// no token was found at any of the configured locations, or the located token failed to
/// verify — an invalid token is reported the same as an expired one, not surfaced as a 500).
pub fn status(token: Option<&str>) -> LicenseStatus {
    status_from_claims(token.and_then(|t| verify(t).ok()))
}

/// Whether product routes should be gated. Mirrors `status().licensed`, named for call-site
/// clarity in the gateway's middleware.
pub fn is_active(token: Option<&str>) -> bool {
    status(token).licensed
}

/// Locates a raw token string from the environment/filesystem — checked in order: explicit env
/// value, then an env-pointed file, then a default file path next to the process's working
/// directory. Shared by the gateway's request-time gating (`atlas-gateway::license`) and
/// `Config::validate_for_start`'s startup warning, so both agree on exactly where a token can
/// come from. Re-resolved on every call (never cached) so replacing the token file takes effect
/// without a restart.
pub fn locate_token_from_env() -> Option<String> {
    for var in ["ATLAS_LICENSE_KEY", "ATLAS_TRIAL_TOKEN"] {
        if let Ok(t) = std::env::var(var) {
            let t = t.trim().to_string();
            if !t.is_empty() {
                return Some(t);
            }
        }
    }
    if let Ok(path) = std::env::var("ATLAS_TRIAL_TOKEN_FILE") {
        if let Ok(s) = std::fs::read_to_string(&path) {
            let s = s.trim().to_string();
            if !s.is_empty() {
                return Some(s);
            }
        }
    }
    if let Ok(s) = std::fs::read_to_string("trial.token") {
        let s = s.trim().to_string();
        if !s.is_empty() {
            return Some(s);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Duration, Utc};
    use jsonwebtoken::{encode, EncodingKey, Header};
    use ring::signature::{Ed25519KeyPair, KeyPair};

    /// Fresh keypair per test — never the real embedded key — plus a helper to mint a claims
    /// token signed by it, so tests don't depend on (or risk leaking) the real signing key.
    struct TestKey {
        pub_b64: String,
        encoding: EncodingKey,
    }

    fn test_keypair() -> TestKey {
        let rng = ring::rand::SystemRandom::new();
        let pkcs8 = Ed25519KeyPair::generate_pkcs8(&rng).expect("keygen");
        let pair = Ed25519KeyPair::from_pkcs8(pkcs8.as_ref()).expect("parse pkcs8");
        let pub_b64 =
            base64::engine::general_purpose::STANDARD.encode(pair.public_key().as_ref());
        TestKey { pub_b64, encoding: EncodingKey::from_ed_der(pkcs8.as_ref()) }
    }

    fn sign(key: &TestKey, sub: &str, product: &str, exp_offset: Duration) -> String {
        let now = Utc::now();
        let claims = LicenseClaims {
            sub: sub.to_string(),
            iat: now.timestamp(),
            exp: (now + exp_offset).timestamp(),
            product: product.to_string(),
        };
        encode(&Header::new(Algorithm::EdDSA), &claims, &key.encoding).expect("sign")
    }

    #[test]
    fn valid_token_verifies_and_reports_active() {
        let key = test_keypair();
        let token = sign(&key, "Acme Corp", PRODUCT_TAG, Duration::days(30));
        let claims = verify_with_key(&token, &key.pub_b64).expect("should verify");
        assert_eq!(claims.sub, "Acme Corp");

        let status = status_from_claims(Some(claims));
        assert!(status.licensed);
        assert!(status.trial_active);
        assert!(!status.trial_expired);
        assert!(status.trial_days_remaining > 0);
    }

    #[test]
    fn wrong_product_tag_is_rejected() {
        let key = test_keypair();
        let token = sign(&key, "Acme Corp", "veyron-trial", Duration::days(30));
        let err = verify_with_key(&token, &key.pub_b64).unwrap_err();
        assert!(err.contains("not issued for this product"));
    }

    #[test]
    fn expired_token_still_decodes_but_reports_inactive_and_expired() {
        // verify_with_key deliberately does NOT reject on expiry (see its doc comment) — an
        // expired-but-authentically-signed token must decode so status() can tell "expired" apart
        // from "never had a token at all". The 402 gate is status()/is_active(), not this Ok/Err.
        let key = test_keypair();
        let token = sign(&key, "Acme Corp", PRODUCT_TAG, Duration::days(-1));
        let claims = verify_with_key(&token, &key.pub_b64).expect("should still decode");
        assert_eq!(claims.sub, "Acme Corp");

        let status = status_from_claims(Some(claims));
        assert!(!status.licensed);
        assert!(!status.trial_active);
        assert!(status.trial_expired, "an expired token must be distinguishable from no token");
        assert_eq!(status.trial_days_remaining, 0);
    }

    #[test]
    fn tampered_signature_is_rejected() {
        let key = test_keypair();
        let other_key = test_keypair();
        let token = sign(&key, "Acme Corp", PRODUCT_TAG, Duration::days(30));
        // Verifying against a *different* public key than the one that signed it must fail —
        // this is what actually proves signature verification is enforced, not just shape.
        let err = verify_with_key(&token, &other_key.pub_b64).unwrap_err();
        assert!(err.contains("invalid"));
    }

    #[test]
    fn no_token_reports_unlicensed_not_expired() {
        let status = status(None);
        assert!(!status.licensed);
        assert!(!status.trial_active);
        assert!(!status.trial_expired);
        assert_eq!(status.trial_days_remaining, 0);
    }

    #[test]
    fn expired_status_is_never_confused_with_missing_status() {
        // Regression guard for the exact bug this crate almost shipped with: LicenseBanner.tsx
        // (crates/atlas-gateway/ui) decides what to render purely from trial_expired vs
        // trial_active, both false meaning "say nothing" — so an expired token reporting
        // trial_expired:false (indistinguishable from never having had one) would silently make
        // the "your trial has ended" banner never appear. Assert the two statuses actually differ.
        let missing = status_from_claims(None);
        let expired = status_from_claims(Some(LicenseClaims {
            sub: "Acme Corp".into(),
            iat: 0,
            exp: 1,
            product: PRODUCT_TAG.into(),
        }));
        assert_ne!(missing.trial_expired, expired.trial_expired);
        assert!(expired.trial_expired);
    }

    #[test]
    fn embedded_public_key_constant_is_valid_base64() {
        // Guards against shipping a placeholder that can never verify anything.
        assert!(base64::engine::general_purpose::STANDARD
            .decode(LICENSE_PUBLIC_KEY_B64)
            .is_ok());
    }
}
