// Copyright (c) 2026 ZyvorAI Labs Private Limited. All rights reserved.
//! Day-2 volume lifecycle: orphan GC surfaces backups whose source volume is gone, and per-image
//! QoS enqueues a throttle job. Fake driver — the QoS job fails without a real `rbd`, but the
//! enqueue contract + validation are exercised; orphan detection is pure-DB and fully verified.

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
        "{}/atlas-lifecycle-{}-{}.db",
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
        jwt_secret: "lifecycle-test-secret-at-least-32-bytes".into(),
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
        rook_namespace: "rook-ceph".into(),
        rook_cluster_name: "rook-ceph".into(),
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

/// A backup whose source volume no longer exists is reported as an orphan; a live-volume backup isn't.
#[tokio::test]
async fn orphan_backups_are_reported() {
    let (addr, pool) = spawn().await;
    let base = format!("http://{addr}/api/atlas/v1");
    let c = reqwest::Client::new();

    atlas_inventory::buckets::insert_bucket(&pool, "bkt1", "t", "b", "ns", "obc", "zyvor-rgw-bucket")
        .await
        .unwrap();
    // A live volume + its backup (not an orphan).
    sqlx::query(
        "INSERT INTO storage_volumes (id, tenant_id, backend_id, name, kind, size_bytes, state)
         VALUES ('live_vol', 't', 'bkd_ceph_lab', 'live', 'block', 1073741824, 'bound')",
    )
    .execute(&pool)
    .await
    .unwrap();
    atlas_inventory::backups::insert_backup(&pool, "bk_live", "t", "live_vol", None, "bkt1", "k/live", "manifest-v1", &json!({}))
        .await
        .unwrap();
    // A backup whose source volume is gone (orphan).
    atlas_inventory::backups::insert_backup(&pool, "bk_orphan", "t", "gone_vol", None, "bkt1", "k/orphan", "manifest-v1", &json!({}))
        .await
        .unwrap();

    let orphans: Value = c
        .get(format!("{base}/maintenance/orphans"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(orphans["count"], 1, "exactly one orphan expected: {orphans}");
    let ids: Vec<&str> = orphans["orphan_backups"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|b| b["id"].as_str())
        .collect();
    assert_eq!(ids, vec!["bk_orphan"]);
}

/// Per-image QoS enqueues a throttle job and validates its inputs.
#[tokio::test]
async fn qos_enqueues_and_validates() {
    let base = format!("http://{}/api/atlas/v1", spawn().await.0);
    let c = reqwest::Client::new();

    // Valid: an IOPS cap → 202 with the limits echoed.
    let ok = c.post(format!("{base}/rbd-images/nvme/img1/qos?iops=1000")).send().await.unwrap();
    assert_eq!(ok.status(), 202);
    let body: Value = ok.json().await.unwrap();
    assert_eq!(body["resource"]["iops_limit"], 1000);
    assert!(body["job_id"].is_string());

    // No limits → 400; negative → 400.
    assert_eq!(c.post(format!("{base}/rbd-images/nvme/img1/qos")).send().await.unwrap().status(), 400);
    assert_eq!(c.post(format!("{base}/rbd-images/nvme/img1/qos?bps=-1")).send().await.unwrap().status(), 400);
}

/// Resize-down is opt-in (allow_shrink), and pool migration validates its destination.
#[tokio::test]
async fn resize_down_and_migrate() {
    let base = format!("http://{}/api/atlas/v1", spawn().await.0);
    let c = reqwest::Client::new();

    // Omitting allow_shrink defaults to false — the opt-in itself must default closed.
    let default_shrink = c
        .post(format!("{base}/rbd-images/nvme/img1/resize"))
        .json(&json!({ "size_bytes": 1073741824i64 }))
        .send()
        .await
        .unwrap();
    assert_eq!(default_shrink.status(), 202);
    assert_eq!(
        default_shrink.json::<Value>().await.unwrap()["resource"]["allow_shrink"], false,
        "allow_shrink must default to false when omitted"
    );

    // Shrink with allow_shrink → 202, echoed.
    let shrink = c
        .post(format!("{base}/rbd-images/nvme/img1/resize"))
        .json(&json!({ "size_bytes": 1073741824i64, "allow_shrink": true }))
        .send()
        .await
        .unwrap();
    assert_eq!(shrink.status(), 202);
    assert_eq!(shrink.json::<Value>().await.unwrap()["resource"]["allow_shrink"], true);

    // Migrate to another pool → 202; same pool or missing dest → 400.
    assert_eq!(c.post(format!("{base}/rbd-images/nvme/img1/migrate?dest_pool=hdd")).send().await.unwrap().status(), 202);
    assert_eq!(c.post(format!("{base}/rbd-images/nvme/img1/migrate?dest_pool=nvme")).send().await.unwrap().status(), 400);
    assert_eq!(c.post(format!("{base}/rbd-images/nvme/img1/migrate")).send().await.unwrap().status(), 400);
}
