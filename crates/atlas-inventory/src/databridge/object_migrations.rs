// Copyright (c) 2026 ZyvorAI Labs Private Limited. All rights reserved.
//! Object-storage migration records (cloud object store -> Ceph RGW). Mirrors the
//! `plans.rs` insert/set/get/list pattern. Credentials are never stored — only secret refs.

use anyhow::Result;
use atlas_api_types::ObjectMigration;
use sqlx::{Row, SqlitePool};

/// Fields the caller supplies at create time. Grouped to avoid a 15-argument function.
#[derive(Debug, Clone)]
pub struct NewObjectMigration {
    pub id: String,
    pub tenant_id: String,
    pub name: String,
    pub source_provider: String,
    pub source_endpoint: String,
    pub source_region: String,
    pub source_bucket: String,
    pub source_prefix: Option<String>,
    pub source_secret_ref: Option<String>,
    pub dest_provider: String,
    pub dest_endpoint: String,
    pub dest_region: String,
    pub dest_bucket: String,
    pub dest_secret_ref: Option<String>,
    pub secret_namespace: String,
    pub mode: String,
}

pub async fn insert(pool: &SqlitePool, m: &NewObjectMigration) -> Result<()> {
    sqlx::query(
        "INSERT INTO object_migrations
         (id, tenant_id, name, source_provider, source_endpoint, source_region, source_bucket,
          source_prefix, source_secret_ref, dest_provider, dest_endpoint, dest_region,
          dest_bucket, dest_secret_ref, secret_namespace, mode, state)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 'created')",
    )
    .bind(&m.id)
    .bind(&m.tenant_id)
    .bind(&m.name)
    .bind(&m.source_provider)
    .bind(&m.source_endpoint)
    .bind(&m.source_region)
    .bind(&m.source_bucket)
    .bind(&m.source_prefix)
    .bind(&m.source_secret_ref)
    .bind(&m.dest_provider)
    .bind(&m.dest_endpoint)
    .bind(&m.dest_region)
    .bind(&m.dest_bucket)
    .bind(&m.dest_secret_ref)
    .bind(&m.secret_namespace)
    .bind(&m.mode)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn set_state(pool: &SqlitePool, id: &str, state: &str) -> Result<()> {
    sqlx::query(
        "UPDATE object_migrations SET state=?, updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE id=?",
    )
    .bind(state)
    .bind(id)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn set_job(pool: &SqlitePool, id: &str, job_id: &str) -> Result<()> {
    sqlx::query(
        "UPDATE object_migrations SET job_id=?, updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE id=?",
    )
    .bind(job_id)
    .bind(id)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn set_totals(pool: &SqlitePool, id: &str, objects: i64, bytes: i64) -> Result<()> {
    sqlx::query(
        "UPDATE object_migrations SET objects_total=?, bytes_total=?,
         updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE id=?",
    )
    .bind(objects)
    .bind(bytes)
    .bind(id)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn set_progress(pool: &SqlitePool, id: &str, objects_done: i64, bytes_done: i64) -> Result<()> {
    sqlx::query(
        "UPDATE object_migrations SET objects_done=?, bytes_done=?,
         updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE id=?",
    )
    .bind(objects_done)
    .bind(bytes_done)
    .bind(id)
    .execute(pool)
    .await?;
    Ok(())
}

/// Terminal outcome: verified + final state, clearing/setting the error.
pub async fn finish(pool: &SqlitePool, id: &str, state: &str, verified: bool, error: Option<&str>) -> Result<()> {
    sqlx::query(
        "UPDATE object_migrations SET state=?, verified=?, last_error=?,
         updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE id=?",
    )
    .bind(state)
    .bind(if verified { 1 } else { 0 })
    .bind(error)
    .bind(id)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn get(pool: &SqlitePool, id: &str) -> Result<Option<ObjectMigration>> {
    let row = sqlx::query(&select("WHERE id = ?"))
        .bind(id)
        .fetch_optional(pool)
        .await?;
    Ok(row.map(row_to))
}

pub async fn list(pool: &SqlitePool, tenant_id: Option<&str>) -> Result<Vec<ObjectMigration>> {
    let rows = match tenant_id {
        Some(t) => {
            sqlx::query(&select("WHERE tenant_id = ? ORDER BY created_at DESC"))
                .bind(t)
                .fetch_all(pool)
                .await?
        }
        None => {
            sqlx::query(&select("ORDER BY created_at DESC"))
                .fetch_all(pool)
                .await?
        }
    };
    Ok(rows.into_iter().map(row_to).collect())
}

pub async fn delete(pool: &SqlitePool, id: &str) -> Result<()> {
    sqlx::query("DELETE FROM object_migrations WHERE id=?")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

fn select(tail: &str) -> String {
    format!(
        "SELECT id, tenant_id, name, source_provider, source_endpoint, source_region,
                source_bucket, source_prefix, source_secret_ref, dest_provider, dest_endpoint,
                dest_region, dest_bucket, dest_secret_ref, secret_namespace, mode, state,
                objects_total, objects_done, bytes_total, bytes_done, verified, last_error,
                job_id, created_at, updated_at
         FROM object_migrations {tail}"
    )
}

fn row_to(r: sqlx::sqlite::SqliteRow) -> ObjectMigration {
    let verified: i64 = r.get("verified");
    ObjectMigration {
        id: r.get("id"),
        tenant_id: r.get("tenant_id"),
        name: r.get("name"),
        source_provider: r.get("source_provider"),
        source_endpoint: r.get("source_endpoint"),
        source_region: r.get("source_region"),
        source_bucket: r.get("source_bucket"),
        source_prefix: r.get("source_prefix"),
        source_secret_ref: r.get("source_secret_ref"),
        dest_provider: r.get("dest_provider"),
        dest_endpoint: r.get("dest_endpoint"),
        dest_region: r.get("dest_region"),
        dest_bucket: r.get("dest_bucket"),
        dest_secret_ref: r.get("dest_secret_ref"),
        secret_namespace: r.get("secret_namespace"),
        mode: r.get("mode"),
        state: r.get("state"),
        objects_total: r.get("objects_total"),
        objects_done: r.get("objects_done"),
        bytes_total: r.get("bytes_total"),
        bytes_done: r.get("bytes_done"),
        verified: verified != 0,
        last_error: r.get("last_error"),
        job_id: r.get("job_id"),
        created_at: r.get("created_at"),
        updated_at: r.get("updated_at"),
    }
}
