// Copyright (c) 2026 ZyvorAI Labs Private Limited. All rights reserved.
//! End-to-end tests for the read-only gateway surface, driven by the FakeCephDriver (no Ceph, no
//! Kubernetes). Verifies discovery populates inventory and that audit rows are written.

use std::net::SocketAddr;

use atlas_common::config::CephDriverMode;
use atlas_common::Config;
use atlas_gateway::routes;
use atlas_gateway::startup::{build_state, BuildOptions};

/// Spin up the gateway on an ephemeral port with the fake driver and a throwaway SQLite file.
async fn spawn() -> (SocketAddr, sqlx::SqlitePool) {
    let db = format!(
        "{}/atlas-test-{}-{}.db",
        std::env::temp_dir().display(),
        std::process::id(),
        NEXT.fetch_add(1, std::sync::atomic::Ordering::SeqCst),
    );
    let _ = std::fs::remove_file(&db);

    let config = Config {
        bind_addr: "127.0.0.1:0".into(),
        database_url: format!("sqlite://{db}?mode=rwc"),
        ceph_driver_mode: CephDriverMode::Fake,
        kubeconfig_path: None,
        jwt_secret: "test-secret".into(),
        auth_required: false,
    };

    let state = build_state(
        config,
        BuildOptions {
            enable_k8s: false,
            initial_discovery: false,
        },
    )
    .await
    .expect("build_state");
    let pool = state.pool.clone();

    let app = routes::router(state);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    (addr, pool)
}

static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

fn client() -> reqwest::Client {
    reqwest::Client::new()
}

#[tokio::test]
async fn health_and_version() {
    let (addr, _pool) = spawn().await;
    let base = format!("http://{addr}");

    let v: serde_json::Value = client()
        .get(format!("{base}/health"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(v["status"], "ok");

    let v: serde_json::Value = client()
        .get(format!("{base}/version"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(v["name"], "atlas-gateway");
}

#[tokio::test]
async fn discovery_populates_inventory_and_audit() {
    let (addr, pool) = spawn().await;
    let base = format!("http://{addr}");

    // The Ceph backend is registered at startup.
    let backends: serde_json::Value = client()
        .get(format!("{base}/api/atlas/v1/backends"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(backends.as_array().unwrap().len(), 1);
    assert_eq!(backends[0]["id"], "bkd_ceph_lab");

    // Trigger discovery.
    let resp = client()
        .post(format!(
            "{base}/api/atlas/v1/backends/bkd_ceph_lab/discover"
        ))
        .send()
        .await
        .unwrap();
    assert!(resp.status().is_success());
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["summary"]["pools"], 3);
    assert_eq!(body["summary"]["volumes"], 2);

    // Inventory is now populated.
    let pools: serde_json::Value = client()
        .get(format!("{base}/api/atlas/v1/pools"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(pools.as_array().unwrap().len(), 3);

    let volumes: serde_json::Value = client()
        .get(format!("{base}/api/atlas/v1/volumes"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(volumes.as_array().unwrap().len(), 2);

    let clusters: serde_json::Value = client()
        .get(format!("{base}/api/atlas/v1/clusters"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(clusters.as_array().unwrap().len(), 1);
    let cluster_id = clusters[0]["id"].as_str().unwrap();

    // Cluster health + capabilities resolve.
    let health: serde_json::Value = client()
        .get(format!("{base}/api/atlas/v1/clusters/{cluster_id}/health"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(health["status"], "ok");

    let caps: serde_json::Value = client()
        .get(format!(
            "{base}/api/atlas/v1/clusters/{cluster_id}/capabilities"
        ))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(caps["block"], true);
    assert_eq!(caps["object"], true);

    // Metrics summary aggregates capacity.
    let metrics: serde_json::Value = client()
        .get(format!("{base}/api/atlas/v1/metrics/summary"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(metrics["clusters"], 1);
    assert_eq!(metrics["pools"], 3);

    // An audit row was written for the discovery request.
    let audits = atlas_inventory::audit::count_for_action(&pool, "backend.discover.requested")
        .await
        .unwrap();
    assert!(audits >= 1, "expected discovery audit row, got {audits}");
}

#[tokio::test]
async fn missing_volume_returns_404() {
    let (addr, _pool) = spawn().await;
    let base = format!("http://{addr}");
    let resp = client()
        .get(format!("{base}/api/atlas/v1/volumes/does-not-exist"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), reqwest::StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn storage_classes_without_cluster_returns_502() {
    // enable_k8s=false → no live driver → /storage-classes should 502, not panic.
    let (addr, _pool) = spawn().await;
    let base = format!("http://{addr}");
    let resp = client()
        .get(format!("{base}/api/atlas/v1/storage-classes"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), reqwest::StatusCode::BAD_GATEWAY);
}
