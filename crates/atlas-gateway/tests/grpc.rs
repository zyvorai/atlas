// Copyright (c) 2026 ZyvorAI Labs Private Limited. All rights reserved.
//! gRPC edge test: start the tonic server over a fake-driver AppState and drive it with the
//! generated client (Health, CreateVolume → job, ListPools).

use atlas_common::config::CephDriverMode;
use atlas_common::Config;
use atlas_gateway::proto::atlas_storage_client::AtlasStorageClient;
use atlas_gateway::proto::{CreateVolumeRequest, Empty, HealthRequest};
use atlas_gateway::startup::{build_state, BuildOptions};

#[tokio::test]
async fn grpc_edge_health_create_and_list() {
    let db = format!(
        "{}/atlas-grpc-test-{}.db",
        std::env::temp_dir().display(),
        std::process::id()
    );
    let _ = std::fs::remove_file(&db);
    let config = Config {
        bind_addr: "127.0.0.1:0".into(),
        grpc_addr: "127.0.0.1:0".into(),
        database_url: format!("sqlite://{db}?mode=rwc"),
        ceph_driver_mode: CephDriverMode::Fake,
        kubeconfig_path: None,
        jwt_secret: "test".into(),
        auth_required: false,
        monitor_interval_secs: 0,
        ceph_prometheus_url: None,
    };
    let state = build_state(
        config,
        BuildOptions {
            enable_k8s: false,
            initial_discovery: true, // populate fake inventory so ListPools is non-empty
            enable_monitor: false,
        },
    )
    .await
    .unwrap();

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        tonic::transport::Server::builder()
            .add_service(atlas_gateway::grpc::service(state))
            .serve_with_incoming(tokio_stream::wrappers::TcpListenerStream::new(listener))
            .await
            .unwrap();
    });

    let mut client = AtlasStorageClient::connect(format!("http://{addr}"))
        .await
        .expect("connect");

    // Health.
    let h = client.health(HealthRequest {}).await.unwrap().into_inner();
    assert_eq!(h.status, "ok");

    // ListPools — the fake driver's initial discovery produced 3 pools.
    let pools = client.list_pools(Empty {}).await.unwrap().into_inner();
    assert_eq!(pools.pools.len(), 3);

    // CreateVolume → returns a job id (job fails later without a cluster, but the RPC contract holds).
    let created = client
        .create_volume(CreateVolumeRequest {
            tenant_id: "t".into(),
            name: "grpc-vol".into(),
            size_bytes: 1_073_741_824,
            policy: "database".into(),
            namespace: "default".into(),
        })
        .await
        .unwrap()
        .into_inner();
    assert!(!created.job_id.is_empty());
    assert!(!created.volume_id.is_empty());
}

#[tokio::test]
async fn grpc_enforces_jwt_when_auth_required() {
    use atlas_gateway::auth::Claims;
    use jsonwebtoken::{encode, EncodingKey, Header};
    use tonic::Request;

    let db = format!(
        "{}/atlas-grpc-auth-{}.db",
        std::env::temp_dir().display(),
        std::process::id()
    );
    let _ = std::fs::remove_file(&db);
    let secret = "grpc-test-secret";
    let config = Config {
        bind_addr: "127.0.0.1:0".into(),
        grpc_addr: "127.0.0.1:0".into(),
        database_url: format!("sqlite://{db}?mode=rwc"),
        ceph_driver_mode: CephDriverMode::Fake,
        kubeconfig_path: None,
        jwt_secret: secret.into(),
        auth_required: true,
        monitor_interval_secs: 0,
        ceph_prometheus_url: None,
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

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        tonic::transport::Server::builder()
            .add_service(atlas_gateway::grpc::service(state))
            .serve_with_incoming(tokio_stream::wrappers::TcpListenerStream::new(listener))
            .await
            .unwrap();
    });

    let mut client = AtlasStorageClient::connect(format!("http://{addr}"))
        .await
        .unwrap();

    // No token → Unauthenticated.
    let err = client.health(HealthRequest {}).await.unwrap_err();
    assert_eq!(err.code(), tonic::Code::Unauthenticated);

    // Valid HS256 token → OK.
    let exp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as usize
        + 3600;
    let claims = Claims {
        sub: "alice".into(),
        role: "admin".into(),
        exp,
    };
    let token = encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(secret.as_bytes()),
    )
    .unwrap();
    let mut req = Request::new(HealthRequest {});
    req.metadata_mut()
        .insert("authorization", format!("Bearer {token}").parse().unwrap());
    let ok = client.health(req).await.unwrap().into_inner();
    assert_eq!(ok.status, "ok");

    // Garbage token → Unauthenticated.
    let mut bad = Request::new(HealthRequest {});
    bad.metadata_mut()
        .insert("authorization", "Bearer not-a-jwt".parse().unwrap());
    assert_eq!(
        client.health(bad).await.unwrap_err().code(),
        tonic::Code::Unauthenticated
    );
}
