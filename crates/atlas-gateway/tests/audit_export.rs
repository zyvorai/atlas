// Copyright (c) 2026 ZyvorAI Labs Private Limited. All rights reserved.
//! Regression guard for the audit-log export-before-prune hook: rows are only ever deleted after
//! a successful export to the configured sink, never unconditionally, so a SIEM outage can't
//! silently lose audit history.

use std::sync::{Arc, Mutex};

use axum::{extract::State, routing::post, Json, Router};
use sqlx::Row;

/// A tiny fake SIEM webhook: captures the last POSTed body and always returns 200.
async fn spawn_fake_sink() -> (std::net::SocketAddr, Arc<Mutex<Option<serde_json::Value>>>) {
    let received: Arc<Mutex<Option<serde_json::Value>>> = Arc::new(Mutex::new(None));
    let state = received.clone();
    let app = Router::new()
        .route(
            "/",
            post(|State(s): State<Arc<Mutex<Option<serde_json::Value>>>>, Json(body): Json<serde_json::Value>| async move {
                *s.lock().unwrap() = Some(body);
                axum::http::StatusCode::OK
            }),
        )
        .with_state(state);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    (addr, received)
}

async fn spawn_db() -> sqlx::SqlitePool {
    let db = format!(
        "{}/atlas-audit-export-{}-{}.db",
        std::env::temp_dir().display(),
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos(),
    );
    let _ = std::fs::remove_file(&db);
    let pool = atlas_inventory::connect_sqlite(&format!("sqlite://{db}?mode=rwc"))
        .await
        .unwrap();
    atlas_inventory::migrate(&pool).await.unwrap();
    pool
}

async fn insert_old_row(pool: &sqlx::SqlitePool, resource_id: &str) {
    sqlx::query(
        "INSERT INTO storage_audit_logs (actor_id, action, resource_type, resource_id, status, created_at)
         VALUES ('me', 'old.action', 'res', ?, 'ok', '2000-01-01T00:00:00.000Z')",
    )
    .bind(resource_id)
    .execute(pool)
    .await
    .unwrap();
}

#[tokio::test]
async fn successful_export_deletes_rows() {
    let pool = spawn_db().await;
    insert_old_row(&pool, "r-success").await;
    let (addr, received) = spawn_fake_sink().await;
    let url = format!("http://{addr}/");

    let n = atlas_monitor::audit_export::export_and_prune(&pool, 30, &url)
        .await
        .unwrap();
    assert!(n >= 1, "should have exported+pruned at least the seeded row");

    let body = received.lock().unwrap().clone().expect("sink should have received a POST");
    let logs = body["audit_logs"].as_array().expect("audit_logs array");
    assert!(logs.iter().any(|r| r["resource_id"] == "r-success"));

    let remaining: i64 = sqlx::query("SELECT COUNT(*) AS c FROM storage_audit_logs WHERE resource_id = 'r-success'")
        .fetch_one(&pool)
        .await
        .unwrap()
        .get("c");
    assert_eq!(remaining, 0, "exported row must be deleted");
}

#[tokio::test]
async fn failed_export_keeps_rows() {
    let pool = spawn_db().await;
    insert_old_row(&pool, "r-kept").await;
    // Nothing listening on this port — the POST will fail to connect.
    let unreachable_url = "http://127.0.0.1:1/";

    let result = atlas_monitor::audit_export::export_and_prune(&pool, 30, unreachable_url).await;
    assert!(result.is_err(), "export to an unreachable sink must error, not silently succeed");

    let remaining: i64 = sqlx::query("SELECT COUNT(*) AS c FROM storage_audit_logs WHERE resource_id = 'r-kept'")
        .fetch_one(&pool)
        .await
        .unwrap()
        .get("c");
    assert_eq!(remaining, 1, "a failed export must not delete the row — retried next tick");
}
