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
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let client = reqwest::Client::new();
    let base = cli.base_url.trim_end_matches('/');

    let (method, path) = match &cli.command {
        Command::Health => ("GET", "/health".to_string()),
        Command::Version => ("GET", "/version".to_string()),
        Command::Backends => ("GET", "/api/atlas/v1/backends".to_string()),
        Command::Discover { backend } => {
            ("POST", format!("/api/atlas/v1/backends/{backend}/discover"))
        }
        Command::Clusters => ("GET", "/api/atlas/v1/clusters".to_string()),
        Command::Pools => ("GET", "/api/atlas/v1/pools".to_string()),
        Command::Osds => ("GET", "/api/atlas/v1/osds".to_string()),
        Command::Volumes => ("GET", "/api/atlas/v1/volumes".to_string()),
        Command::StorageClasses => ("GET", "/api/atlas/v1/storage-classes".to_string()),
        Command::Metrics => ("GET", "/api/atlas/v1/metrics/summary".to_string()),
    };

    let url = format!("{base}{path}");
    let mut req = match method {
        "POST" => client.post(&url),
        _ => client.get(&url),
    };
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
