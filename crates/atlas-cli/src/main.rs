// Copyright (c) 2026 ZyvorAI Labs Private Limited. All rights reserved.
//! `atlasctl` — a thin REST client for the Atlas gateway.

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "atlasctl", about = "Atlas storage control-plane CLI")]
struct Cli {
    /// Gateway base URL.
    #[arg(long, env = "ATLAS_BASE_URL", default_value = "http://127.0.0.1:5110")]
    base_url: String,

    /// Optional bearer token (when the gateway has auth enabled).
    #[arg(long, env = "ATLAS_TOKEN")]
    token: Option<String>,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// GET /health
    Health,
    /// GET /version
    Version,
    /// GET /api/atlas/v1/backends
    Backends,
    /// POST /api/atlas/v1/backends/{id}/discover (default backend: bkd_ceph_lab)
    Discover {
        #[arg(default_value = "bkd_ceph_lab")]
        backend: String,
    },
    /// GET /api/atlas/v1/clusters
    Clusters,
    /// GET /api/atlas/v1/pools
    Pools,
    /// GET /api/atlas/v1/osds
    Osds,
    /// GET /api/atlas/v1/volumes
    Volumes,
    /// GET /api/atlas/v1/storage-classes (live from Kubernetes)
    StorageClasses,
    /// GET /api/atlas/v1/metrics/summary
    Metrics,
    /// GET /api/atlas/v1/alerts (optionally filter by --state open|resolved)
    Alerts {
        #[arg(long)]
        state: Option<String>,
    },
    /// GET /api/atlas/v1/metrics/ceph (latest scraped Ceph metrics; --prefix to filter)
    CephMetrics {
        #[arg(long)]
        prefix: Option<String>,
    },
    /// GET /api/atlas/v1/policies
    Policies,
    /// GET /api/atlas/v1/jobs  (or a single job with an id)
    Jobs { id: Option<String> },
    /// GET /api/atlas/v1/snapshots
    Snapshots,
    /// POST /api/atlas/v1/volumes — create a Ceph-backed volume (PVC)
    CreateVolume {
        name: String,
        /// Size in GiB.
        #[arg(long, default_value_t = 2)]
        size_gib: i64,
        #[arg(long, default_value = "database")]
        policy: String,
        #[arg(long, default_value = "default")]
        namespace: String,
        #[arg(long, default_value = "tenant_default")]
        tenant: String,
    },
    /// POST /api/atlas/v1/volumes/{id}/snapshots
    SnapshotVolume {
        volume_id: String,
        #[arg(long)]
        name: Option<String>,
    },
    /// DELETE /api/atlas/v1/volumes/{id}
    DeleteVolume { id: String },
    /// POST /api/atlas/v1/snapshots/{id}/clone
    CloneSnapshot {
        snapshot_id: String,
        #[arg(long)]
        name: String,
        #[arg(long)]
        namespace: Option<String>,
    },
    /// POST /api/atlas/v1/snapshots/{id}/restore
    RestoreSnapshot {
        snapshot_id: String,
        #[arg(long)]
        name: Option<String>,
    },
    /// DELETE /api/atlas/v1/snapshots/{id}  (use --force if it has dependents)
    DeleteSnapshot {
        id: String,
        #[arg(long)]
        force: bool,
    },
    /// GET /api/atlas/v1/buckets
    Buckets,
    /// POST /api/atlas/v1/buckets — provision an RGW bucket
    CreateBucket {
        name: String,
        #[arg(long)]
        namespace: Option<String>,
    },
    /// POST /api/atlas/v1/backup-jobs — back up a volume to a bucket
    BackupVolume {
        volume_id: String,
        #[arg(long)]
        bucket_id: String,
        /// "manifest" (default) or "data" (also exports RBD image data to S3).
        #[arg(long, default_value = "manifest")]
        mode: String,
    },
    /// GET /api/atlas/v1/backups
    Backups,
    /// POST /api/atlas/v1/restore-jobs — restore a volume from a backup
    RestoreBackup {
        backup_id: String,
        #[arg(long)]
        name: Option<String>,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let client = reqwest::Client::new();
    let base = cli.base_url.trim_end_matches('/');

    let (method, path, body): (&str, String, Option<serde_json::Value>) = match &cli.command {
        Command::Health => ("GET", "/health".to_string(), None),
        Command::Version => ("GET", "/version".to_string(), None),
        Command::Backends => ("GET", "/api/atlas/v1/backends".to_string(), None),
        Command::Discover { backend } => (
            "POST",
            format!("/api/atlas/v1/backends/{backend}/discover"),
            None,
        ),
        Command::Clusters => ("GET", "/api/atlas/v1/clusters".to_string(), None),
        Command::Pools => ("GET", "/api/atlas/v1/pools".to_string(), None),
        Command::Osds => ("GET", "/api/atlas/v1/osds".to_string(), None),
        Command::Volumes => ("GET", "/api/atlas/v1/volumes".to_string(), None),
        Command::StorageClasses => ("GET", "/api/atlas/v1/storage-classes".to_string(), None),
        Command::Metrics => ("GET", "/api/atlas/v1/metrics/summary".to_string(), None),
        Command::Alerts { state } => (
            "GET",
            match state {
                Some(st) => format!("/api/atlas/v1/alerts?state={st}"),
                None => "/api/atlas/v1/alerts".to_string(),
            },
            None,
        ),
        Command::CephMetrics { prefix } => (
            "GET",
            match prefix {
                Some(p) => format!("/api/atlas/v1/metrics/ceph?prefix={p}"),
                None => "/api/atlas/v1/metrics/ceph".to_string(),
            },
            None,
        ),
        Command::Policies => ("GET", "/api/atlas/v1/policies".to_string(), None),
        Command::Jobs { id } => match id {
            Some(id) => ("GET", format!("/api/atlas/v1/jobs/{id}"), None),
            None => ("GET", "/api/atlas/v1/jobs".to_string(), None),
        },
        Command::Snapshots => ("GET", "/api/atlas/v1/snapshots".to_string(), None),
        Command::CreateVolume {
            name,
            size_gib,
            policy,
            namespace,
            tenant,
        } => (
            "POST",
            "/api/atlas/v1/volumes".to_string(),
            Some(serde_json::json!({
                "tenant_id": tenant,
                "name": name,
                "size_bytes": size_gib * 1024 * 1024 * 1024,
                "kind": "block",
                "policy": policy,
                "kubernetes": { "namespace": namespace, "create_pvc": true }
            })),
        ),
        Command::SnapshotVolume { volume_id, name } => (
            "POST",
            format!("/api/atlas/v1/volumes/{volume_id}/snapshots"),
            Some(serde_json::json!({ "name": name })),
        ),
        Command::DeleteVolume { id } => ("DELETE", format!("/api/atlas/v1/volumes/{id}"), None),
        Command::CloneSnapshot {
            snapshot_id,
            name,
            namespace,
        } => (
            "POST",
            format!("/api/atlas/v1/snapshots/{snapshot_id}/clone"),
            Some(serde_json::json!({ "name": name, "namespace": namespace })),
        ),
        Command::RestoreSnapshot { snapshot_id, name } => (
            "POST",
            format!("/api/atlas/v1/snapshots/{snapshot_id}/restore"),
            Some(serde_json::json!({ "name": name })),
        ),
        Command::DeleteSnapshot { id, force } => (
            "DELETE",
            format!("/api/atlas/v1/snapshots/{id}?force={force}"),
            None,
        ),
        Command::Buckets => ("GET", "/api/atlas/v1/buckets".to_string(), None),
        Command::CreateBucket { name, namespace } => (
            "POST",
            "/api/atlas/v1/buckets".to_string(),
            Some(serde_json::json!({ "name": name, "namespace": namespace })),
        ),
        Command::BackupVolume {
            volume_id,
            bucket_id,
            mode,
        } => (
            "POST",
            "/api/atlas/v1/backup-jobs".to_string(),
            Some(
                serde_json::json!({ "volume_id": volume_id, "bucket_id": bucket_id, "mode": mode }),
            ),
        ),
        Command::Backups => ("GET", "/api/atlas/v1/backups".to_string(), None),
        Command::RestoreBackup { backup_id, name } => (
            "POST",
            "/api/atlas/v1/restore-jobs".to_string(),
            Some(serde_json::json!({ "backup_id": backup_id, "name": name })),
        ),
    };

    let url = format!("{base}{path}");
    let mut req = match method {
        "POST" => client.post(&url),
        "DELETE" => client.delete(&url),
        _ => client.get(&url),
    };
    if let Some(b) = &body {
        req = req.json(b);
    }
    if let Some(token) = &cli.token {
        req = req.bearer_auth(token);
    }

    let resp = req
        .send()
        .await
        .with_context(|| format!("request to {url}"))?;
    let status = resp.status();
    let body = resp.text().await.unwrap_or_default();

    // Pretty-print JSON when possible.
    match serde_json::from_str::<serde_json::Value>(&body) {
        Ok(v) => println!("{}", serde_json::to_string_pretty(&v)?),
        Err(_) => println!("{body}"),
    }

    if !status.is_success() {
        anyhow::bail!("request failed: HTTP {status}");
    }
    Ok(())
}
