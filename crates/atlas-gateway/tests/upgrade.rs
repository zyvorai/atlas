// Copyright (c) 2026 ZyvorAI Labs Private Limited. All rights reserved.
//! Day-2 upgrade orchestration: the pre-flight health gate. A quiet control plane is ready to
//! upgrade; in-flight jobs and open critical alerts block it. Fake driver, no infra.

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
        "{}/atlas-upgrade-{}-{}.db",
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
        jwt_secret: "upgrade-test-secret-at-least-32-bytes!!".into(),
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

async fn preflight(c: &reqwest::Client, addr: SocketAddr) -> Value {
    c.get(format!("http://{addr}/api/atlas/v1/upgrade/preflight"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap()
}

/// A quiet control plane (no jobs, no critical alerts, healthy) is ready to upgrade.
#[tokio::test]
async fn preflight_ready_when_quiet() {
    let (addr, _) = spawn().await;
    let pf = preflight(&reqwest::Client::new(), addr).await;
    assert_eq!(pf["ready"], true, "{pf}");
    assert!(pf["blockers"].as_array().unwrap().is_empty());
    // all four checks reported.
    assert_eq!(pf["checks"].as_array().unwrap().len(), 4);
}

/// An in-flight job and an open critical alert each block the upgrade.
#[tokio::test]
async fn preflight_blocks_on_active_job_and_critical_alert() {
    let (addr, pool) = spawn().await;

    // Seed a job in a non-terminal state directly (state the boot recovery + worker don't touch, so
    // it stays "in flight" for the assertion rather than being drained by the engine).
    sqlx::query(
        "INSERT INTO storage_jobs (id, tenant_id, job_type, state, requested_by, request)
         VALUES ('j', 't', 'volume.create', 'verifying', 'me', '{}')",
    )
    .execute(&pool)
    .await
    .unwrap();
    atlas_inventory::alerts::upsert_open(
        &pool, "a1", "critical", "monitor", "cluster", "c1", "Cluster down", "HEALTH_ERR", &json!({}),
    )
    .await
    .unwrap();

    let pf = preflight(&reqwest::Client::new(), addr).await;
    assert_eq!(pf["ready"], false, "{pf}");
    let blockers: Vec<&str> = pf["blockers"].as_array().unwrap().iter().filter_map(|b| b.as_str()).collect();
    assert!(blockers.iter().any(|b| b.contains("job")), "job blocker: {blockers:?}");
    assert!(blockers.iter().any(|b| b.contains("critical alert")), "alert blocker: {blockers:?}");
}
