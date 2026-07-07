// Copyright (c) 2026 ZyvorAI Labs Private Limited. All rights reserved.
//! Object-storage bucket records (PDF §9.9, §11 storage_buckets).

use anyhow::Result;
use atlas_api_types::StorageBucket;
use sqlx::{Row, SqlitePool};

/// Insert a pending bucket row (before the OBC binds).
pub async fn insert_bucket(
    pool: &SqlitePool,
    id: &str,
    tenant_id: &str,
    name: &str,
    namespace: &str,
    obc_name: &str,
) -> Result<()> {
    sqlx::query(
        "INSERT INTO storage_buckets (id, tenant_id, name, namespace, obc_name, state)
         VALUES (?, ?, ?, ?, ?, 'pending')",
    )
    .bind(id)
    .bind(tenant_id)
    .bind(name)
    .bind(namespace)
    .bind(obc_name)
    .execute(pool)
    .await?;
    Ok(())
}

/// Fill in the bucket details once the OBC is bound.
#[allow(clippy::too_many_arguments)]
pub async fn set_bound(
    pool: &SqlitePool,
    id: &str,
    bucket_name: &str,
    endpoint: &str,
    region: &str,
    secret_ref: &str,
) -> Result<()> {
    sqlx::query(
        "UPDATE storage_buckets SET bucket_name=?, endpoint=?, region=?, secret_ref=?, state='bound' WHERE id=?",
    )
    .bind(bucket_name)
    .bind(endpoint)
    .bind(region)
    .bind(secret_ref)
    .bind(id)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn get_bucket(pool: &SqlitePool, id: &str) -> Result<Option<StorageBucket>> {
    let row = sqlx::query(&select("WHERE id = ?"))
        .bind(id)
        .fetch_optional(pool)
        .await?;
    Ok(row.map(row_to_bucket))
}

pub async fn list_buckets(pool: &SqlitePool) -> Result<Vec<StorageBucket>> {
    let rows = sqlx::query(&select("ORDER BY created_at DESC"))
        .fetch_all(pool)
        .await?;
    Ok(rows.into_iter().map(row_to_bucket).collect())
}

fn select(tail: &str) -> String {
    format!(
        "SELECT id, tenant_id, name, bucket_name, endpoint, region, secret_ref, namespace, state, created_at
         FROM storage_buckets {tail}"
    )
}

fn row_to_bucket(r: sqlx::sqlite::SqliteRow) -> StorageBucket {
    StorageBucket {
        id: r.get("id"),
        tenant_id: r.get("tenant_id"),
        name: r.get("name"),
        bucket_name: r.get("bucket_name"),
        endpoint: r.get("endpoint"),
        region: r.get("region"),
        secret_ref: r.get("secret_ref"),
        namespace: r.get("namespace"),
        state: r.get("state"),
        created_at: r.get("created_at"),
    }
}
