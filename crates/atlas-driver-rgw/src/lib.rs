// Copyright (c) 2026 ZyvorAI Labs Private Limited. All rights reserved.
//! Minimal S3 client for Ceph RGW (PDF §10.3 "Atlas → RGW: S3 API").
//!
//! Uses `rusty-s3` to build SigV4-signed request URLs and `reqwest` to execute them — no heavy
//! AWS SDK. Path-style addressing (required by RGW). Credentials are passed in by the caller
//! (read from the Rook OBC Secret in-cluster); this crate never persists or logs them.

use std::time::Duration;

use anyhow::{Context, Result};
use futures_util::StreamExt;
use rusty_s3::actions::{CreateMultipartUpload, ListObjectsV2};
use rusty_s3::{Bucket, Credentials, S3Action, UrlStyle};
use sha2::{Digest, Sha256};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

const SIGN_TTL: Duration = Duration::from_secs(300);

/// S3 minimum part size (5 MiB). Every multipart part except the last must be at least this.
pub const MIN_PART_SIZE: usize = 5 * 1024 * 1024;

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

    /// A time-limited presigned PUT URL for `key` (client can upload directly to RGW without
    /// credentials — the gateway never sees the object bytes). SigV4 query auth, so the browser
    /// just does `fetch(url, { method: "PUT", body: file })`.
    pub fn presigned_put(&self, key: &str, ttl_secs: u64) -> String {
        self.bucket
            .put_object(Some(&self.creds), key)
            .sign(Duration::from_secs(ttl_secs))
            .to_string()
    }

    /// Stream `src` to `key` as an S3 multipart upload, hashing the bytes as they pass through.
    /// Returns `(total_bytes, sha256_hex)`. Never buffers the whole object — only one `part_size`
    /// chunk at a time (clamped to the S3 5 MiB minimum). Aborts the upload on any error.
    pub async fn put_multipart_streaming<R>(
        &self,
        key: &str,
        part_size: usize,
        mut src: R,
    ) -> Result<(u64, String)>
    where
        R: AsyncRead + Unpin,
    {
        let part_size = part_size.max(MIN_PART_SIZE);
        let action = self.bucket.create_multipart_upload(Some(&self.creds), key);
        let resp = self
            .http
            .post(action.sign(SIGN_TTL))
            .send()
            .await
            .with_context(|| format!("initiate multipart {key}"))?;
        if !resp.status().is_success() {
            let s = resp.status();
            let d = resp.text().await.unwrap_or_default();
            anyhow::bail!("initiate multipart {key} failed: HTTP {s}: {d}");
        }
        let body = resp.text().await?;
        let upload_id = CreateMultipartUpload::parse_response(&body)
            .context("parse multipart initiate response")?
            .upload_id()
            .to_string();

        match self
            .upload_all_parts(key, &upload_id, part_size, &mut src)
            .await
        {
            Ok((total, sha, etags)) => {
                let action = self.bucket.complete_multipart_upload(
                    Some(&self.creds),
                    key,
                    &upload_id,
                    etags.iter().map(String::as_str),
                );
                let signed = action.sign(SIGN_TTL);
                let xml = action.body();
                let resp = self
                    .http
                    .post(signed)
                    .body(xml)
                    .send()
                    .await
                    .with_context(|| format!("complete multipart {key}"))?;
                if !resp.status().is_success() {
                    let s = resp.status();
                    let d = resp.text().await.unwrap_or_default();
                    let _ = self.abort_multipart(key, &upload_id).await;
                    anyhow::bail!("complete multipart {key} failed: HTTP {s}: {d}");
                }
                Ok((total, sha))
            }
            Err(e) => {
                let _ = self.abort_multipart(key, &upload_id).await;
                Err(e)
            }
        }
    }

    async fn upload_all_parts<R>(
        &self,
        key: &str,
        upload_id: &str,
        part_size: usize,
        src: &mut R,
    ) -> Result<(u64, String, Vec<String>)>
    where
        R: AsyncRead + Unpin,
    {
        let mut hasher = Sha256::new();
        let mut etags: Vec<String> = Vec::new();
        let mut total: u64 = 0;
        let mut part_number: u16 = 1;
        let mut buf = vec![0u8; part_size];
        loop {
            // Fill a full part before uploading (a short read only means "not yet EOF").
            let mut filled = 0;
            while filled < part_size {
                let n = src
                    .read(&mut buf[filled..])
                    .await
                    .context("read part from source")?;
                if n == 0 {
                    break;
                }
                filled += n;
            }
            if filled == 0 {
                break;
            }
            hasher.update(&buf[..filled]);
            total += filled as u64;
            let etag = self
                .upload_one_part(key, upload_id, part_number, buf[..filled].to_vec())
                .await?;
            etags.push(etag);
            part_number += 1;
            if filled < part_size {
                break; // reached EOF on this part
            }
        }
        // S3 requires at least one part; upload an empty final part for an empty source.
        if etags.is_empty() {
            let etag = self.upload_one_part(key, upload_id, 1, Vec::new()).await?;
            etags.push(etag);
        }
        Ok((total, hex::encode(hasher.finalize()), etags))
    }

    async fn upload_one_part(
        &self,
        key: &str,
        upload_id: &str,
        part_number: u16,
        body: Vec<u8>,
    ) -> Result<String> {
        let action = self
            .bucket
            .upload_part(Some(&self.creds), key, part_number, upload_id);
        let resp = self
            .http
            .put(action.sign(SIGN_TTL))
            .body(body)
            .send()
            .await
            .with_context(|| format!("upload part {part_number} of {key}"))?;
        if !resp.status().is_success() {
            let s = resp.status();
            let d = resp.text().await.unwrap_or_default();
            anyhow::bail!("upload part {part_number} of {key} failed: HTTP {s}: {d}");
        }
        resp.headers()
            .get(reqwest::header::ETAG)
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string())
            .ok_or_else(|| anyhow::anyhow!("upload part {part_number} of {key}: missing ETag"))
    }

    async fn abort_multipart(&self, key: &str, upload_id: &str) -> Result<()> {
        let action = self
            .bucket
            .abort_multipart_upload(Some(&self.creds), key, upload_id);
        let _ = self.http.delete(action.sign(SIGN_TTL)).send().await;
        Ok(())
    }

    /// Stream `key` from S3 into `sink`, hashing the bytes as they pass through. Returns
    /// `(total_bytes, sha256_hex)`. Never buffers the whole object in memory.
    pub async fn get_object_streaming<W>(&self, key: &str, mut sink: W) -> Result<(u64, String)>
    where
        W: AsyncWrite + Unpin,
    {
        let action = self.bucket.get_object(Some(&self.creds), key);
        let resp = self
            .http
            .get(action.sign(SIGN_TTL))
            .send()
            .await
            .with_context(|| format!("GET {key}"))?;
        if !resp.status().is_success() {
            anyhow::bail!("GET {key} failed: HTTP {}", resp.status());
        }
        let mut hasher = Sha256::new();
        let mut total: u64 = 0;
        let mut stream = resp.bytes_stream();
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.with_context(|| format!("stream body {key}"))?;
            hasher.update(&chunk);
            total += chunk.len() as u64;
            sink.write_all(&chunk)
                .await
                .context("write streamed object to sink")?;
        }
        sink.flush().await.ok();
        Ok((total, hex::encode(hasher.finalize())))
    }

    /// List objects in the bucket (optionally under `prefix`) as `(key, size_bytes)` pairs.
    pub async fn list_objects(&self, prefix: Option<&str>) -> Result<Vec<(String, u64)>> {
        let mut action = self.bucket.list_objects_v2(Some(&self.creds));
        if let Some(p) = prefix {
            action.query_mut().insert("prefix", p.to_owned());
        }
        let resp = self
            .http
            .get(action.sign(SIGN_TTL))
            .send()
            .await
            .context("list objects")?;
        if !resp.status().is_success() {
            anyhow::bail!("list objects failed: HTTP {}", resp.status());
        }
        let body = resp.text().await?;
        let parsed = ListObjectsV2::parse_response(&body).context("parse list objects")?;
        Ok(parsed
            .contents
            .into_iter()
            .map(|c| (c.key, c.size))
            .collect())
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
