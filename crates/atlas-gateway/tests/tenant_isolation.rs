// Copyright (c) 2026 ZyvorAI Labs Private Limited. All rights reserved.
//! Regression guard for the cross-tenant read-isolation fix: a viewer-role JWT scoped to one
//! tenant must never see another tenant's volumes/buckets via list, get, or CSV export, while
//! admin stays cross-tenant by design. Seeds two tenants' worth of resources directly into
//! inventory (bypassing the job engine — this test is about the *read* path, not provisioning).

use std::net::SocketAddr;

use atlas_common::config::CephDriverMode;
use atlas_common::Config;
use atlas_gateway::routes;
use atlas_gateway::startup::{build_state, BuildOptions};

static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// Spawn a gateway with auth enabled; returns its base URL and a handle to the inventory pool for
/// seeding fixtures directly.
async fn spawn_auth(secret: &str) -> (SocketAddr, sqlx::SqlitePool) {
    let db = format!(
        "{}/atlas-tenant-iso-{}-{}.db",
        std::env::temp_dir().display(),
        std::process::id(),
        NEXT.fetch_add(1, std::sync::atomic::Ordering::SeqCst),
    );
    let _ = std::fs::remove_file(&db);
    let config = Config {
        bind_addr: "127.0.0.1:0".into(),
        grpc_addr: "127.0.0.1:0".into(),
        database_url: format!("sqlite://{db}?mode=rwc"),
        ceph_driver_mode: CephDriverMode::Fake,
        kubeconfig_path: None,
        jwt_secret: secret.into(),
        jwt_secret_previous: None,
        auth_required: true,
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
        license_enforce: false,
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
    .unwrap();
    let pool = state.pool.clone();
    let app = routes::router(state);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    (addr, pool)
}

async fn seed_volume(pool: &sqlx::SqlitePool, id: &str, tenant_id: &str, name: &str) {
    let v = atlas_api_types::StorageVolume {
        id: id.into(),
        cluster_id: None,
        pool_id: None,
        name: name.into(),
        kind: atlas_api_types::VolumeKind::Block,
        backend_native_id: Some(format!("rbd:pool/{id}")),
        size_bytes: 1073741824,
        used_bytes: None,
        state: "bound".into(),
        health: atlas_api_types::Health::Ok,
        kubernetes_namespace: Some("default".into()),
        pvc_name: Some(name.into()),
        storage_class_name: Some("zyvor-rbd-prod".into()),
    };
    atlas_inventory::upsert_volume(pool, "bkd_ceph_lab", tenant_id, &v, None)
        .await
        .unwrap();
}

async fn seed_bucket(pool: &sqlx::SqlitePool, id: &str, tenant_id: &str, name: &str) {
    atlas_inventory::buckets::insert_bucket(pool, id, tenant_id, name, "rook-ceph", name, "zyvor-rgw-bucket")
        .await
        .unwrap();
}

fn client() -> reqwest::Client {
    reqwest::Client::new()
}

/// Two tenants, one volume + one bucket each; a viewer scoped to tenant A must only ever see
/// tenant A's resources across list/get/CSV, an explicit `?tenant=` override must not escape the
/// scope, and admin must see both tenants regardless.
#[tokio::test]
async fn viewer_is_scoped_to_own_tenant() {
    let secret = "tenant-iso-test-secret-key-32-bytes!!";
    let (addr, pool) = spawn_auth(secret).await;
    let base = format!("http://{addr}/api/atlas/v1");
    let c = client();

    seed_volume(&pool, "vol_tenant_a", "tenant-a", "a-vol").await;
    seed_volume(&pool, "vol_tenant_b", "tenant-b", "b-vol").await;
    seed_bucket(&pool, "bkt_tenant_a", "tenant-a", "a-bucket").await;
    seed_bucket(&pool, "bkt_tenant_b", "tenant-b", "b-bucket").await;

    let (viewer_a, _, _) =
        atlas_gateway::auth::mint_token(secret, "alice", "viewer", "tenant-a", 3600).unwrap();
    let (viewer_b, _, _) =
        atlas_gateway::auth::mint_token(secret, "bob", "viewer", "tenant-b", 3600).unwrap();
    let (admin, _, _) =
        atlas_gateway::auth::mint_token(secret, "root", "admin", "global", 3600).unwrap();

    // --- list_volumes ---
    let vols_a: Vec<serde_json::Value> = c
        .get(format!("{base}/volumes"))
        .bearer_auth(&viewer_a)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(vols_a.len(), 1, "tenant A viewer should see exactly 1 volume");
    assert_eq!(vols_a[0]["id"], "vol_tenant_a");

    let vols_b: Vec<serde_json::Value> = c
        .get(format!("{base}/volumes"))
        .bearer_auth(&viewer_b)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(vols_b.len(), 1, "tenant B viewer should see exactly 1 volume");
    assert_eq!(vols_b[0]["id"], "vol_tenant_b");

    let vols_admin: Vec<serde_json::Value> = c
        .get(format!("{base}/volumes"))
        .bearer_auth(&admin)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(vols_admin.len(), 2, "admin should see both tenants' volumes");

    // --- explicit ?tenant= override attempt must not escape scope ---
    let vols_a_probe: Vec<serde_json::Value> = c
        .get(format!("{base}/volumes?tenant=tenant-b"))
        .bearer_auth(&viewer_a)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(
        vols_a_probe.len(),
        1,
        "tenant A viewer passing ?tenant=tenant-b must still only see tenant A's own volume"
    );
    assert_eq!(vols_a_probe[0]["id"], "vol_tenant_a");

    // --- get_volume: cross-tenant fetch by id is 404, not the resource ---
    let cross = c
        .get(format!("{base}/volumes/vol_tenant_b"))
        .bearer_auth(&viewer_a)
        .send()
        .await
        .unwrap();
    assert_eq!(cross.status(), 404, "tenant A viewer must not fetch tenant B's volume by id");

    let own = c
        .get(format!("{base}/volumes/vol_tenant_a"))
        .bearer_auth(&viewer_a)
        .send()
        .await
        .unwrap();
    assert_eq!(own.status(), 200, "tenant A viewer must fetch their own volume");

    let admin_cross = c
        .get(format!("{base}/volumes/vol_tenant_b"))
        .bearer_auth(&admin)
        .send()
        .await
        .unwrap();
    assert_eq!(admin_cross.status(), 200, "admin must fetch any tenant's volume");

    // --- volumes.csv must not leak the other tenant's row ---
    let csv_a = c
        .get(format!("{base}/volumes.csv"))
        .bearer_auth(&viewer_a)
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(csv_a.contains("vol_tenant_a"), "CSV must contain the caller's own volume");
    assert!(
        !csv_a.contains("vol_tenant_b"),
        "CSV must not leak another tenant's volume row"
    );

    // --- list_buckets / get_bucket, same shape ---
    let bkts_a: Vec<serde_json::Value> = c
        .get(format!("{base}/buckets"))
        .bearer_auth(&viewer_a)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(bkts_a.len(), 1);
    assert_eq!(bkts_a[0]["id"], "bkt_tenant_a");

    let bkt_cross = c
        .get(format!("{base}/buckets/bkt_tenant_b"))
        .bearer_auth(&viewer_a)
        .send()
        .await
        .unwrap();
    assert_eq!(bkt_cross.status(), 404, "tenant A viewer must not fetch tenant B's bucket by id");

    let bkts_admin: Vec<serde_json::Value> = c
        .get(format!("{base}/buckets"))
        .bearer_auth(&admin)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(bkts_admin.len(), 2, "admin should see both tenants' buckets");
}
