// Copyright (c) 2026 ZyvorAI Labs Private Limited. All rights reserved.
//! Minimal S3 client for Ceph RGW (PDF §10.3 "Atlas → RGW: S3 API").
//!
//! Uses `rusty-s3` to build SigV4-signed request URLs and `reqwest` to execute them — no heavy
//! AWS SDK. Path-style addressing (required by RGW). Credentials are passed in by the caller
//! (read from the Rook OBC Secret in-cluster); this crate never persists or logs them.

use std::time::Duration;

use anyhow::{Context, Result};
use rusty_s3::{Bucket, Credentials, S3Action, UrlStyle};

const SIGN_TTL: Duration = Duration::from_secs(300);

/// A handle to one bucket on an S3/RGW endpoint.
pub struct S3Target {
    bucket: Bucket,
    creds: Credentials,
    http: reqwest::Client,
}

impl S3Target {
    /// Build from an endpoint URL (`http://host:port`), region, bucket name, and credentials.
    pub fn new(
        endpoint: &str,
        region: &str,
        bucket_name: &str,
        access_key: &str,
        secret_key: &str,
    ) -> Result<Self> {
        let url: url::Url = endpoint.parse().context("invalid S3 endpoint URL")?;
        let region = if region.is_empty() {
            "us-east-1"
        } else {
            region
        };
        let bucket = Bucket::new(
            url,
            UrlStyle::Path,
            bucket_name.to_string(),
            region.to_string(),
        )
        .context("invalid bucket")?;
        Ok(Self {
            bucket,
            creds: Credentials::new(access_key, secret_key),
            http: reqwest::Client::new(),
        })
    }

    /// PUT an object.
    pub async fn put_object(&self, key: &str, body: Vec<u8>) -> Result<()> {
        let action = self.bucket.put_object(Some(&self.creds), key);
        let signed = action.sign(SIGN_TTL);
        let resp = self
            .http
            .put(signed)
            .body(body)
            .send()
            .await
            .with_context(|| format!("PUT {key}"))?;
        let status = resp.status();
        if !status.is_success() {
            let detail = resp.text().await.unwrap_or_default();
            anyhow::bail!("PUT {key} failed: HTTP {status}: {detail}");
        }
        Ok(())
    }

    /// GET an object's bytes.
    pub async fn get_object(&self, key: &str) -> Result<Vec<u8>> {
        let action = self.bucket.get_object(Some(&self.creds), key);
        let signed = action.sign(SIGN_TTL);
        let resp = self
            .http
            .get(signed)
            .send()
            .await
            .with_context(|| format!("GET {key}"))?;
        let status = resp.status();
        if !status.is_success() {
            anyhow::bail!("GET {key} failed: HTTP {status}");
        }
        Ok(resp.bytes().await?.to_vec())
    }

    /// Whether an object exists (via GET; RGW/reqwest keep this simple for the MVP).
    pub async fn object_exists(&self, key: &str) -> bool {
        self.get_object(key).await.is_ok()
    }

    /// A time-limited presigned GET URL for `key` (client can download without credentials).
    pub fn presigned_get(&self, key: &str, ttl_secs: u64) -> String {
        self.bucket
            .get_object(Some(&self.creds), key)
            .sign(Duration::from_secs(ttl_secs))
            .to_string()
    }

    /// DELETE an object. S3 delete is idempotent (deleting a missing key returns success).
    pub async fn delete_object(&self, key: &str) -> Result<()> {
        let action = self.bucket.delete_object(Some(&self.creds), key);
        let signed = action.sign(SIGN_TTL);
        let resp = self
            .http
            .delete(signed)
            .send()
            .await
            .with_context(|| format!("DELETE {key}"))?;
        let status = resp.status();
        // 204/200 on success; 404 is fine (already gone).
        if !status.is_success() && status.as_u16() != 404 {
            let detail = resp.text().await.unwrap_or_default();
            anyhow::bail!("DELETE {key} failed: HTTP {status}: {detail}");
        }
        Ok(())
    }
}
