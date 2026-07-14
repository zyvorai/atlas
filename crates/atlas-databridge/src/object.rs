// Copyright (c) 2026 ZyvorAI Labs Private Limited. All rights reserved.

//! Object-storage migration — the *object* leg of Atlas DataBridge (AWS S3 → Ceph RGW).
//!
//! Mirrors the database DataBridge shape (discover → copy → verify) but for S3 objects.
//! Reuses [`atlas_driver_rgw::S3Target`] as the byte-mover for BOTH endpoints: the AWS
//! source and the RGW destination are both S3-compatible (SigV4, path-style), so one
//! client type serves both. The copy is:
//!
//!   list source → diff vs dest (full | incremental by key+size) → stream each object,
//!   recording its sha256 → verify every planned key landed on the destination.
//!
//! Credentials are never logged: [`S3Endpoint`] does not derive `Debug` over its secret
//! fields (see [`S3Endpoint::redacted`]).

use anyhow::{Context, Result};
use atlas_driver_rgw::S3Target;
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use std::collections::HashMap;

/// The object-store provider behind an endpoint. The S3-protocol family (AWS S3, Google
/// Cloud Storage via its S3-interoperability HMAC keys, and any S3-compatible store such as
/// MinIO / Wasabi / DigitalOcean Spaces / Ceph RGW) all move bytes through [`S3Target`]
/// today. `AzureBlob` and `Vmware` are recognized so the API/model is multi-cloud from day
/// one, but they require their own connectors (not in this build) — mirroring how the DB
/// DataBridge gates oracle/mongodb/sqlserver behind cargo features.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ObjectProvider {
    Aws,
    Gcs,
    S3Compatible,
    AzureBlob,
    Vmware,
}

impl ObjectProvider {
    pub fn parse(s: &str) -> Self {
        match s.to_ascii_lowercase().as_str() {
            "aws" | "aws-s3" | "s3" => ObjectProvider::Aws,
            "gcs" | "gcp" | "google" | "google-cloud-storage" => ObjectProvider::Gcs,
            "azure" | "azure-blob" | "abs" => ObjectProvider::AzureBlob,
            "vmware" | "vsphere" | "vsan" => ObjectProvider::Vmware,
            _ => ObjectProvider::S3Compatible,
        }
    }

    /// True when this provider speaks the S3 protocol and can use [`S3Target`] directly.
    pub fn is_s3_protocol(self) -> bool {
        matches!(
            self,
            ObjectProvider::Aws | ObjectProvider::Gcs | ObjectProvider::S3Compatible
        )
    }
}

/// Connection to one S3-compatible endpoint (AWS source or Ceph RGW destination).
///
/// Deliberately does NOT derive `Debug`/`Serialize` — the access/secret keys must never
/// reach a log line or a persisted record. Use [`redacted`](Self::redacted) for display.
#[derive(Clone, Deserialize)]
pub struct S3Endpoint {
    /// Full endpoint URL, e.g. `https://s3.us-east-1.amazonaws.com` (AWS) or the RGW URL.
    pub endpoint: String,
    #[serde(default)]
    pub region: String,
    pub bucket: String,
    /// Optional key prefix to scope the migration to a subtree of the bucket.
    #[serde(default)]
    pub prefix: Option<String>,
    pub access_key: String,
    pub secret_key: String,
}

impl S3Endpoint {
    /// Build an [`S3Target`] byte-mover for this endpoint.
    pub fn target(&self) -> Result<S3Target> {
        S3Target::new(
            &self.endpoint,
            &self.region,
            &self.bucket,
            &self.access_key,
            &self.secret_key,
        )
        .with_context(|| format!("build S3 client for {}", self.redacted()))
    }

    /// Credential-free identifier safe to log (`endpoint/bucket[/prefix]`).
    pub fn redacted(&self) -> String {
        match &self.prefix {
            Some(p) if !p.is_empty() => format!("{}/{}/{}", self.endpoint, self.bucket, p),
            _ => format!("{}/{}", self.endpoint, self.bucket),
        }
    }
}

/// How much to copy.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CopyMode {
    /// Copy every source object, overwriting the destination.
    Full,
    /// Copy only objects missing from the destination or whose size differs.
    Incremental,
}

/// One object selected for copy.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct PlannedObject {
    pub key: String,
    pub size: u64,
}

/// Live progress, emitted after each object copies.
#[derive(Clone, Debug, Default, Serialize)]
pub struct CopyProgress {
    pub objects_total: usize,
    pub objects_done: usize,
    pub bytes_total: u64,
    pub bytes_done: u64,
}

/// Terminal report of a copy run.
#[derive(Clone, Debug, Default, Serialize)]
pub struct CopyReport {
    pub objects_copied: usize,
    pub bytes_copied: u64,
    /// `(key, sha256_hex)` recorded from the source stream — the migration's audit trail.
    pub checksums: Vec<(String, String)>,
    /// True once every planned key was confirmed present on the destination at the right size.
    pub verified: bool,
    pub verify_error: Option<String>,
}

/// Pure copy planner: given source and destination listings (`(key, size)` pairs), decide
/// which objects to copy. No I/O — unit-tested offline.
///
/// * [`CopyMode::Full`] → every source object.
/// * [`CopyMode::Incremental`] → objects absent from the destination, or present but with a
///   different size (a cheap proxy for "changed"; RGW/AWS ETags are not comparable across
///   multipart boundaries, so size is the portable signal for the first slice).
pub fn diff_objects(
    source: &[(String, u64)],
    dest: &[(String, u64)],
    mode: CopyMode,
) -> Vec<PlannedObject> {
    let dest_map: HashMap<&str, u64> = dest.iter().map(|(k, s)| (k.as_str(), *s)).collect();
    source
        .iter()
        .filter(|(k, s)| match mode {
            CopyMode::Full => true,
            CopyMode::Incremental => dest_map.get(k.as_str()).map_or(true, |ds| ds != s),
        })
        .map(|(k, s)| PlannedObject {
            key: k.clone(),
            size: *s,
        })
        .collect()
}

/// A read-only object-store source. The destination is always an [`S3Target`] (Ceph RGW),
/// but the source is pluggable so non-S3 clouds (Azure Blob, native GCS, …) can migrate in
/// without touching the copy/verify logic. The S3-protocol family uses [`S3ObjectSource`];
/// Azure Blob uses the feature-gated `object_azure::AzureBlobSource`.
#[async_trait::async_trait]
pub trait ObjectSource: Send + Sync {
    /// List `(key, size)` under an optional prefix.
    async fn list(&self, prefix: Option<&str>) -> Result<Vec<(String, u64)>>;
    /// Read an object fully into memory, returning `(bytes, sha256_hex)`.
    async fn get(&self, key: &str) -> Result<(Vec<u8>, String)>;
}

/// S3-protocol source (AWS S3, GCS S3-interop, MinIO/RGW/…): wraps an [`S3Target`].
pub struct S3ObjectSource(pub S3Target);

#[async_trait::async_trait]
impl ObjectSource for S3ObjectSource {
    async fn list(&self, prefix: Option<&str>) -> Result<Vec<(String, u64)>> {
        self.0.list_objects(prefix).await
    }
    async fn get(&self, key: &str) -> Result<(Vec<u8>, String)> {
        let mut buf: Vec<u8> = Vec::new();
        let (_, sha) = self.0.get_object_streaming(key, &mut buf).await?;
        Ok((buf, sha))
    }
}

/// Migrates objects from any [`ObjectSource`] into an S3-compatible (Ceph RGW) destination.
pub struct ObjectMigrator {
    source: Box<dyn ObjectSource>,
    dest: S3Target,
    prefix: Option<String>,
}

impl ObjectMigrator {
    /// Build an S3-source → S3-dest migrator from two endpoints.
    pub fn new(source: &S3Endpoint, dest: &S3Endpoint) -> Result<Self> {
        Ok(Self {
            source: Box::new(S3ObjectSource(source.target()?)),
            dest: dest.target()?,
            prefix: source.prefix.clone(),
        })
    }

    /// Build a migrator from an arbitrary source (e.g. Azure Blob) into an RGW destination.
    pub fn with_source(source: Box<dyn ObjectSource>, dest: S3Target, prefix: Option<String>) -> Self {
        Self { source, dest, prefix }
    }

    /// List both sides and compute the copy plan.
    pub async fn plan(&self, mode: CopyMode) -> Result<Vec<PlannedObject>> {
        let src = self
            .source
            .list(self.prefix.as_deref())
            .await
            .context("list source objects")?;
        let dst = self
            .dest
            .list_objects(self.prefix.as_deref())
            .await
            .context("list destination objects")?;
        Ok(diff_objects(&src, &dst, mode))
    }

    /// Copy a single object, returning `(bytes, sha256_hex)`. Buffers the object in memory
    /// (first slice); large-object streaming pipe is a follow-up.
    async fn copy_one(&self, key: &str) -> Result<(u64, String)> {
        let (buf, sha) = self
            .source
            .get(key)
            .await
            .with_context(|| format!("read source object {key}"))?;
        let bytes = buf.len() as u64;
        self.dest
            .put_object(key, buf)
            .await
            .with_context(|| format!("write destination object {key}"))?;
        Ok((bytes, sha))
    }

    /// Execute `plan`, invoking `on_progress` after each object, then verify.
    pub async fn run<F>(&self, plan: &[PlannedObject], mut on_progress: F) -> Result<CopyReport>
    where
        F: FnMut(&CopyProgress),
    {
        let bytes_total: u64 = plan.iter().map(|p| p.size).sum();
        let mut progress = CopyProgress {
            objects_total: plan.len(),
            bytes_total,
            ..Default::default()
        };
        let mut report = CopyReport::default();
        for obj in plan {
            let (bytes, sha) = self.copy_one(&obj.key).await?;
            report.objects_copied += 1;
            report.bytes_copied += bytes;
            report.checksums.push((obj.key.clone(), sha));
            progress.objects_done = report.objects_copied;
            progress.bytes_done = report.bytes_copied;
            on_progress(&progress);
        }
        match self.verify(plan).await {
            Ok(()) => report.verified = true,
            Err(e) => {
                report.verified = false;
                report.verify_error = Some(e.to_string());
            }
        }
        Ok(report)
    }

    /// Confirm every planned key exists on the destination at the expected size.
    async fn verify(&self, plan: &[PlannedObject]) -> Result<()> {
        let dst = self
            .dest
            .list_objects(self.prefix.as_deref())
            .await
            .context("verify: list destination")?;
        let dmap: HashMap<&str, u64> = dst.iter().map(|(k, s)| (k.as_str(), *s)).collect();
        for obj in plan {
            match dmap.get(obj.key.as_str()) {
                Some(&sz) if sz == obj.size => {}
                Some(&sz) => {
                    anyhow::bail!("size mismatch for {}: expected {}, got {}", obj.key, obj.size, sz)
                }
                None => anyhow::bail!("object missing on destination: {}", obj.key),
            }
        }
        Ok(())
    }
}

/// Resolve S3 credentials from a referenced k8s Secret. Accepts either the plain
/// `access_key`/`secret_key` keys or the AWS-style `AWS_ACCESS_KEY_ID`/
/// `AWS_SECRET_ACCESS_KEY`. Credentials never leave this process and are never logged.
async fn resolve_creds(
    k8s: &atlas_driver_k8s::K8sDriver,
    namespace: &str,
    secret_ref: &str,
) -> Result<(String, String)> {
    let data = k8s
        .get_secret(namespace, secret_ref)
        .await
        .map_err(|e| anyhow::anyhow!("read secret {namespace}/{secret_ref}: {e}"))?
        .ok_or_else(|| anyhow::anyhow!("secret {namespace}/{secret_ref} not found"))?;
    let access = data
        .get("access_key")
        .or_else(|| data.get("AWS_ACCESS_KEY_ID"))
        .cloned()
        .ok_or_else(|| anyhow::anyhow!("secret {secret_ref} missing access_key/AWS_ACCESS_KEY_ID"))?;
    let secret = data
        .get("secret_key")
        .or_else(|| data.get("AWS_SECRET_ACCESS_KEY"))
        .cloned()
        .ok_or_else(|| {
            anyhow::anyhow!("secret {secret_ref} missing secret_key/AWS_SECRET_ACCESS_KEY")
        })?;
    Ok((access, secret))
}

/// Drive a persisted object-migration record to completion: resolve creds → plan →
/// copy (persisting progress) → verify → finalize. Invoked by the job engine's
/// `JobSpec::ObjectMigrate` arm. Errors are recorded on the record as `failed`.
pub async fn run_migration(
    pool: &SqlitePool,
    k8s: Option<&atlas_driver_k8s::K8sDriver>,
    id: &str,
) -> Result<()> {
    use atlas_inventory::databridge::object_migrations as store;

    let rec = store::get(pool, id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("object migration {id} not found"))?;

    // Run the copy, recording a failure on the record if anything goes wrong.
    match run_migration_inner(pool, k8s, &rec).await {
        Ok(verified) => {
            let state = if verified { "completed" } else { "failed" };
            store::finish(pool, id, state, verified, None).await?;
            if verified {
                Ok(())
            } else {
                anyhow::bail!("verification failed for object migration {id}")
            }
        }
        Err(e) => {
            // Record the message but not any credential material (errors here never embed creds).
            let _ = store::finish(pool, id, "failed", false, Some(&e.to_string())).await;
            Err(e)
        }
    }
}

async fn run_migration_inner(
    pool: &SqlitePool,
    k8s: Option<&atlas_driver_k8s::K8sDriver>,
    rec: &atlas_api_types::ObjectMigration,
) -> Result<bool> {
    use atlas_inventory::databridge::object_migrations as store;

    let source_provider = ObjectProvider::parse(&rec.source_provider);
    let dest_provider = ObjectProvider::parse(&rec.dest_provider);

    // The destination is always Ceph RGW (S3 protocol).
    if !dest_provider.is_s3_protocol() {
        anyhow::bail!(
            "destination provider '{}' must be S3-protocol (Ceph RGW is the migration target)",
            rec.dest_provider
        );
    }
    // VMware vSphere/vSAN stores VMs/VMDKs on block storage — it is not an object store, so
    // an object-copy path does not apply. Its volumes migrate via the block (RBD import) leg.
    if source_provider == ObjectProvider::Vmware {
        anyhow::bail!(
            "VMware vSphere/vSAN is not an object store; migrate its volumes/VMs via the block \
             (RBD import) path, not object copy"
        );
    }

    let k8s = k8s.ok_or_else(|| {
        anyhow::anyhow!("object migration requires a Kubernetes driver to resolve credential secrets")
    })?;

    let src_ref = rec
        .source_secret_ref
        .as_deref()
        .ok_or_else(|| anyhow::anyhow!("source_secret_ref is required"))?;
    let dst_ref = rec
        .dest_secret_ref
        .as_deref()
        .ok_or_else(|| anyhow::anyhow!("dest_secret_ref is required"))?;
    let (src_ak, src_sk) = resolve_creds(k8s, &rec.secret_namespace, src_ref).await?;
    let (dst_ak, dst_sk) = resolve_creds(k8s, &rec.secret_namespace, dst_ref).await?;

    // Destination endpoint (Ceph RGW).
    let dest = S3Endpoint {
        endpoint: rec.dest_endpoint.clone(),
        region: rec.dest_region.clone(),
        bucket: rec.dest_bucket.clone(),
        prefix: rec.source_prefix.clone(), // copy preserves keys (incl. prefix) into dest bucket
        access_key: dst_ak,
        secret_key: dst_sk,
    };

    let mode = match rec.mode.as_str() {
        "full" => CopyMode::Full,
        _ => CopyMode::Incremental,
    };

    // Build the migrator. S3-family sources (aws | gcs | s3-compatible) use the S3 path;
    // Azure Blob uses its native, feature-gated connector.
    let migrator = match source_provider {
        ObjectProvider::AzureBlob => {
            #[cfg(feature = "azure-blob")]
            {
                // Azure convention: access_key = storage account name, secret_key = account
                // key, bucket = container name.
                let src = crate::object_azure::AzureBlobSource::new(
                    &src_ak,
                    &src_sk,
                    &rec.source_bucket,
                )?;
                ObjectMigrator::with_source(Box::new(src), dest.target()?, rec.source_prefix.clone())
            }
            #[cfg(not(feature = "azure-blob"))]
            {
                let _ = (&src_ak, &src_sk); // creds resolved, but the connector isn't compiled
                anyhow::bail!(
                    "azure-blob connector is not compiled in this build; rebuild with \
                     --features atlas-databridge/azure-blob"
                );
            }
        }
        _ => {
            let source = S3Endpoint {
                endpoint: rec.source_endpoint.clone(),
                region: rec.source_region.clone(),
                bucket: rec.source_bucket.clone(),
                prefix: rec.source_prefix.clone(),
                access_key: src_ak,
                secret_key: src_sk,
            };
            ObjectMigrator::new(&source, &dest)?
        }
    };

    store::set_state(pool, &rec.id, "planning").await?;
    let plan = migrator.plan(mode).await?;
    let bytes_total: u64 = plan.iter().map(|p| p.size).sum();
    store::set_totals(pool, &rec.id, plan.len() as i64, bytes_total as i64).await?;

    store::set_state(pool, &rec.id, "copying").await?;
    // Persist progress at most every ~2s of wall-time-equivalent (every 16 objects) to keep
    // the record fresh without a DB write per tiny object.
    let mut last_persist = 0usize;
    let report = {
        let pool = pool.clone();
        let id = rec.id.clone();
        migrator
            .run(&plan, |progress| {
                if progress.objects_done - last_persist >= 16
                    || progress.objects_done == progress.objects_total
                {
                    last_persist = progress.objects_done;
                    let pool = pool.clone();
                    let id = id.clone();
                    let done = progress.objects_done as i64;
                    let bytes = progress.bytes_done as i64;
                    // Fire-and-forget progress write; a dropped update is re-sent on the next tick.
                    tokio::spawn(async move {
                        let _ = store::set_progress(&pool, &id, done, bytes).await;
                    });
                }
            })
            .await?
    };

    store::set_state(pool, &rec.id, "verifying").await?;
    store::set_progress(pool, &rec.id, report.objects_copied as i64, report.bytes_copied as i64).await?;
    Ok(report.verified)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn objs(pairs: &[(&str, u64)]) -> Vec<(String, u64)> {
        pairs.iter().map(|(k, s)| (k.to_string(), *s)).collect()
    }

    #[test]
    fn full_copies_everything() {
        let src = objs(&[("a", 10), ("b", 20)]);
        let dst = objs(&[("a", 10)]); // 'a' already present — full ignores that
        let plan = diff_objects(&src, &dst, CopyMode::Full);
        assert_eq!(plan.len(), 2);
        assert_eq!(plan[0].key, "a");
        assert_eq!(plan[1].key, "b");
    }

    #[test]
    fn incremental_skips_identical() {
        let src = objs(&[("a", 10), ("b", 20)]);
        let dst = objs(&[("a", 10), ("b", 20)]);
        assert!(diff_objects(&src, &dst, CopyMode::Incremental).is_empty());
    }

    #[test]
    fn incremental_copies_missing_and_changed() {
        let src = objs(&[("a", 10), ("b", 20), ("c", 30)]);
        let dst = objs(&[("a", 10), ("b", 99)]); // b changed size, c missing
        let plan = diff_objects(&src, &dst, CopyMode::Incremental);
        let keys: Vec<&str> = plan.iter().map(|p| p.key.as_str()).collect();
        assert_eq!(keys, vec!["b", "c"]);
        assert_eq!(plan[0].size, 20); // planned size is the SOURCE size
    }

    #[test]
    fn empty_source_plans_nothing() {
        let dst = objs(&[("a", 10)]);
        assert!(diff_objects(&[], &dst, CopyMode::Full).is_empty());
        assert!(diff_objects(&[], &dst, CopyMode::Incremental).is_empty());
    }

    #[test]
    fn redacted_never_contains_secrets() {
        let ep = S3Endpoint {
            endpoint: "https://s3.us-east-1.amazonaws.com".into(),
            region: "us-east-1".into(),
            bucket: "datasets".into(),
            prefix: Some("models/".into()),
            access_key: "AKIA_SECRET_ID".into(),
            secret_key: "super/secret/key".into(),
        };
        let r = ep.redacted();
        assert!(!r.contains("AKIA_SECRET_ID"));
        assert!(!r.contains("super/secret/key"));
        assert!(r.contains("datasets"));
    }

    #[test]
    fn copy_mode_serde_roundtrip() {
        assert_eq!(
            serde_json::from_str::<CopyMode>("\"incremental\"").unwrap(),
            CopyMode::Incremental
        );
        assert_eq!(
            serde_json::to_string(&CopyMode::Full).unwrap(),
            "\"full\""
        );
    }
}
