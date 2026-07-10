// Copyright (c) 2026 ZyvorAI Labs Private Limited. All rights reserved.
//! Day-2 cluster-ops & maintenance mode: cordoning a backend rejects new provisioning, the global
//! maintenance pause holds jobs until resumed, and OSD ops enqueue as jobs. Fake driver, no infra
//! (the OSD job fails without a real `ceph`, but the enqueue contract + validation are exercised).

use std::net::SocketAddr;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use atlas_common::config::CephDriverMode;
use atlas_common::Config;
use atlas_gateway::routes;
use atlas_gateway::startup::{build_state, BuildOptions};
use serde_json::{json, Value};

static NEXT: AtomicU64 = AtomicU64::new(0);
const CEPH: &str = "bkd_ceph_lab";

async fn spawn() -> SocketAddr {
    let db = format!(
        "{}/atlas-maint-{}-{}.db",
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
        jwt_secret: "maint-test-secret-key-at-least-32-byte".into(),
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
    let app = routes::router(state);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    addr
}

fn vol(name: &str) -> Value {
    json!({
        "tenant_id": "t", "name": name, "size_bytes": 1_073_741_824i64,
        "policy": "database", "kubernetes": { "namespace": "default" }
    })
}

/// A cordoned backend rejects new provisioning (503); uncordon restores it.
#[tokio::test]
async fn cordon_blocks_provisioning() {
    let base = format!("http://{}/api/atlas/v1", spawn().await);
    let c = reqwest::Client::new();

    // Baseline: create accepted (the job fails later without k8s, but enqueue is 202).
    assert_eq!(c.post(format!("{base}/volumes")).json(&vol("v0")).send().await.unwrap().status(), 202);

    // Cordon → new provisioning rejected with 503.
    let cor = c.post(format!("{base}/backends/{CEPH}/cordon")).send().await.unwrap();
    assert_eq!(cor.status(), 200);
    assert_eq!(cor.json::<Value>().await.unwrap()["cordoned"], true);
    assert_eq!(c.post(format!("{base}/volumes")).json(&vol("v1")).send().await.unwrap().status(), 503);

    // Uncordon → provisioning resumes.
    assert_eq!(c.post(format!("{base}/backends/{CEPH}/uncordon")).send().await.unwrap().status(), 200);
    assert_eq!(c.post(format!("{base}/volumes")).json(&vol("v2")).send().await.unwrap().status(), 202);

    // Cordoning an unknown backend → 404.
    assert_eq!(c.post(format!("{base}/backends/nope/cordon")).send().await.unwrap().status(), 404);
}

/// The maintenance pause holds jobs in `queued` until resumed, then they drain.
#[tokio::test]
async fn pause_holds_jobs_until_resumed() {
    let base = format!("http://{}/api/atlas/v1", spawn().await);
    let c = reqwest::Client::new();

    // Enter maintenance.
    let p = c.post(format!("{base}/maintenance")).json(&json!({ "paused": true })).send().await.unwrap();
    assert_eq!(p.status(), 200);
    let m: Value = c.get(format!("{base}/maintenance")).send().await.unwrap().json().await.unwrap();
    assert_eq!(m["paused"], true);

    // Enqueue an OSD op; the worker should hold it (leave it queued) while paused.
    let job: Value = c
        .post(format!("{base}/osds/5/out"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let job_id = job["job_id"].as_str().unwrap().to_string();

    tokio::time::sleep(Duration::from_millis(400)).await;
    let held: Value = c
        .get(format!("{base}/jobs/{job_id}"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert!(
        matches!(held["state"].as_str(), Some("queued") | Some("pending")),
        "job should be held while paused, got {:?}",
        held["state"]
    );

    // Resume → the held job drains to a terminal state (failed here — no real ceph binary).
    assert_eq!(
        c.post(format!("{base}/maintenance")).json(&json!({ "paused": false })).send().await.unwrap().status(),
        200
    );
    let mut last = String::new();
    for _ in 0..40 {
        let j: Value = c.get(format!("{base}/jobs/{job_id}")).send().await.unwrap().json().await.unwrap();
        last = j["state"].as_str().unwrap_or("").to_string();
        if last == "failed" || last == "succeeded" {
            return;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("job did not drain after resume (last state: '{last}')");
}

/// OSD ops enqueue as jobs; reweight validates its weight.
#[tokio::test]
async fn osd_ops_enqueue_and_validate() {
    let base = format!("http://{}/api/atlas/v1", spawn().await);
    let c = reqwest::Client::new();

    let out = c.post(format!("{base}/osds/3/out")).send().await.unwrap();
    assert_eq!(out.status(), 202);
    assert!(out.json::<Value>().await.unwrap()["job_id"].is_string());

    // reweight without a weight → 400; out of range → 400; valid → 202.
    assert_eq!(c.post(format!("{base}/osds/3/reweight")).send().await.unwrap().status(), 400);
    assert_eq!(c.post(format!("{base}/osds/3/reweight?weight=2.0")).send().await.unwrap().status(), 400);
    assert_eq!(c.post(format!("{base}/osds/3/reweight?weight=0.8")).send().await.unwrap().status(), 202);
}
