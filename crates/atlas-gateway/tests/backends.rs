// Copyright (c) 2026 ZyvorAI Labs Private Limited. All rights reserved.
//! `POST /backends` instantiates a live NFS/ZFS driver (not just a catalog row) and discovers it, so
//! its pools appear immediately. Fake driver, no infra (NFS/ZFS are deterministic fixture drivers).

use std::net::SocketAddr;
use std::sync::atomic::{AtomicU64, Ordering};

use atlas_common::config::CephDriverMode;
use atlas_common::Config;
use atlas_gateway::routes;
use atlas_gateway::startup::{build_state, BuildOptions};
use serde_json::{json, Value};

static NEXT: AtomicU64 = AtomicU64::new(0);

async fn spawn() -> SocketAddr {
    let db = format!(
        "{}/atlas-backends-{}-{}.db",
        std::env::temp_dir().display(),
        std::process::id(),
        NEXT.fetch_add(1, Ordering::SeqCst),
    );
    let _ = std::fs::remove_file(&db);
    let config = Config {
        bind_addr: "127.0.0.1:0".into(),
        grpc_addr: "127.0.0.1:0".into(),
        database_url: format!("sqlite://{db}?mode=rwc"),
        ceph_driver_mode: CephDriverMode::Fake,
        kubeconfig_path: None,
        jwt_secret: "backends-test-secret-at-least-32-bytes!".into(),
        jwt_secret_previous: None,
        auth_required: false,
        bootstrap_admin_token: None,
        admin_username: "admin".into(),
        admin_password: "Admin@321".into(),
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
    };
    let state = build_state(
        config,
        BuildOptions { enable_k8s: false, initial_discovery: true, enable_monitor: false },
    )
    .await
    .expect("build_state");
    let app = routes::router(state);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    addr
}

#[tokio::test]
async fn post_backend_registers_live_nfs_driver() {
    let base = format!("http://{}/api/atlas/v1", spawn().await);
    let c = reqwest::Client::new();

    let created: Value = c
        .post(format!("{base}/backends"))
        .json(&json!({ "name": "extra-nfs", "backend_type": "nfs", "server": "nas.lab", "targets": ["/exports/data"] }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(created["status"], "active", "nfs backend should be live, not pending: {created}");
    let bid = created["id"].as_str().unwrap();

    // The newly-registered backend discovered its export as a pool.
    let pools: Value = c.get(format!("{base}/pools")).send().await.unwrap().json().await.unwrap();
    assert!(
        pools.as_array().unwrap().iter().any(|p| p["kind"] == "nfs_export"),
        "the live NFS backend should have discovered an export pool: {pools}"
    );
    assert!(!bid.is_empty());
}
