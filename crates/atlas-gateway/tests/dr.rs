// Copyright (c) 2026 ZyvorAI Labs Private Limited. All rights reserved.
//! Day-2 cross-cluster DR — control-plane scaffolding. Register a peer, enable RBD mirroring on a
//! volume, and fail over (promote/demote). Fake driver: the `rbd mirror` jobs fail without a real
//! second cluster, but the DR catalog + API + role transitions are exercised. Real mirroring is
//! UNVERIFIED here by design (needs a live peer cluster).

use std::net::SocketAddr;
use std::sync::atomic::{AtomicU64, Ordering};

use atlas_common::config::CephDriverMode;
use atlas_common::Config;
use atlas_gateway::routes;
use atlas_gateway::startup::{build_state, BuildOptions};
use serde_json::{json, Value};

static NEXT: AtomicU64 = AtomicU64::new(0);

async fn spawn() -> (SocketAddr, sqlx::SqlitePool) {
    let db = format!(
        "{}/atlas-dr-{}-{}.db",
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
        jwt_secret: "dr-test-secret-at-least-32-bytes-long!!".into(),
        auth_required: false,
        monitor_interval_secs: 0,
        ceph_prometheus_url: None,
        alert_webhook_url: None,
        backup_keep: 0,
        backup_max_age_secs: 0,
        rgw_public_endpoint: None,
        snapshot_tick_secs: 0,
        databridge_reconcile_secs: 0,
        https_addr: None,
        tls_cert_path: None,
        tls_key_path: None,
        tls_self_signed: false,
        nfs_enable: false,
        nfs_server: None,
        nfs_exports: Vec::new(),
        zfs_enable: false,
        zfs_host: None,
        zfs_pools: Vec::new(),
    };
    let state = build_state(
        config,
        BuildOptions {
            enable_k8s: false,
            initial_discovery: false,
            enable_monitor: false,
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

#[tokio::test]
async fn dr_peer_mirror_and_failover() {
    let (addr, pool) = spawn().await;
    let base = format!("http://{addr}/api/atlas/v1");
    let c = reqwest::Client::new();

    // Register a mirroring peer.
    let peer = c
        .post(format!("{base}/dr/peers"))
        .json(&json!({ "name": "dc2", "cluster_fsid": "fsid-2", "secret_ref": "dc2-bootstrap" }))
        .send()
        .await
        .unwrap();
    assert_eq!(peer.status(), 201);
    let peers: Value = c.get(format!("{base}/dr/peers")).send().await.unwrap().json().await.unwrap();
    assert!(peers.as_array().unwrap().iter().any(|p| p["name"] == "dc2"));

    // A direct-RBD volume to mirror.
    sqlx::query(
        "INSERT INTO storage_volumes (id, tenant_id, backend_id, name, kind, size_bytes, state, backend_native_id)
         VALUES ('v1', 't', 'bkd_ceph_lab', 'db', 'block', 1073741824, 'bound', 'rbd:nvme/img1')",
    )
    .execute(&pool)
    .await
    .unwrap();

    // Enable mirroring → recorded (fake reports enabled) with role primary.
    let en = c.post(format!("{base}/volumes/v1/mirror?mode=snapshot&peer=drp_test")).send().await.unwrap();
    assert_eq!(en.status(), 202);
    let mirror_id = en.json::<Value>().await.unwrap()["resource"]["mirror_id"].as_str().unwrap().to_string();

    let mirror = |c: &reqwest::Client| {
        let base = base.clone();
        let c = c.clone();
        let mid = mirror_id.clone();
        async move {
            let list: Value = c.get(format!("{base}/dr/mirrors")).send().await.unwrap().json().await.unwrap();
            list.as_array().unwrap().iter().find(|m| m["id"] == json!(mid)).cloned().unwrap()
        }
    };
    let m = mirror(&c).await;
    assert_eq!(m["role"], "primary");
    assert_eq!(m["state"], "enabled");
    assert_eq!(m["pool"], "nvme");
    assert_eq!(m["image"], "img1");

    // Failover drill: demote → secondary, then promote → primary.
    assert_eq!(c.post(format!("{base}/dr/mirrors/{mirror_id}/demote")).send().await.unwrap().status(), 202);
    assert_eq!(mirror(&c).await["role"], "secondary");
    assert_eq!(c.post(format!("{base}/dr/mirrors/{mirror_id}/promote")).send().await.unwrap().status(), 202);
    assert_eq!(mirror(&c).await["role"], "primary");

    // Promoting an unknown mirror → 404.
    assert_eq!(c.post(format!("{base}/dr/mirrors/nope/promote")).send().await.unwrap().status(), 404);

    // DR status summarizes the posture.
    let status: Value = c.get(format!("{base}/dr/status")).send().await.unwrap().json().await.unwrap();
    assert_eq!(status["peers"], 1);
    assert_eq!(status["mirrors"], 1);
    assert_eq!(status["primary"], 1);
}
