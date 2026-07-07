// Copyright (c) 2026 ZyvorAI Labs Private Limited. All rights reserved.
//! Per-tenant storage quotas + live usage (PDF §14 multi-tenancy). Usage is computed from the live
//! `storage_volumes` rows for the tenant, so it always reflects what is currently provisioned.

use anyhow::Result;
use atlas_api_types::TenantQuota;
use sqlx::{Row, SqlitePool};

/// Current usage for a tenant: total provisioned volume bytes and volume count.
pub async fn usage(pool: &SqlitePool, tenant_id: &str) -> Result<(i64, i64)> {
    let row = sqlx::query(
        "SELECT COALESCE(SUM(size_bytes), 0) AS used, COUNT(*) AS n
         FROM storage_volumes WHERE tenant_id = ?",
    )
    .bind(tenant_id)
    .fetch_one(pool)
    .await?;
    Ok((row.get::<i64, _>("used"), row.get::<i64, _>("n")))
}

/// Fetch a tenant's quota (with current usage). Returns unlimited (0/0) limits if none is set.
pub async fn get_quota(pool: &SqlitePool, tenant_id: &str) -> Result<TenantQuota> {
    let row =
        sqlx::query("SELECT max_bytes, max_volumes FROM storage_tenant_quotas WHERE tenant_id = ?")
            .bind(tenant_id)
            .fetch_optional(pool)
            .await?;
    let (max_bytes, max_volumes) = match row {
        Some(r) => (r.get::<i64, _>("max_bytes"), r.get::<i64, _>("max_volumes")),
        None => (0, 0),
    };
    let (used_bytes, volume_count) = usage(pool, tenant_id).await?;
    Ok(TenantQuota {
        tenant_id: tenant_id.to_string(),
        max_bytes,
        max_volumes,
        used_bytes,
        volume_count,
    })
}

/// Set (upsert) a tenant's quota. `0` for a limit means unlimited.
pub async fn set_quota(
    pool: &SqlitePool,
    tenant_id: &str,
    max_bytes: i64,
    max_volumes: i64,
) -> Result<()> {
    sqlx::query(
        "INSERT INTO storage_tenant_quotas (tenant_id, max_bytes, max_volumes)
         VALUES (?, ?, ?)
         ON CONFLICT(tenant_id) DO UPDATE SET
            max_bytes = excluded.max_bytes,
            max_volumes = excluded.max_volumes,
            updated_at = strftime('%Y-%m-%dT%H:%M:%fZ','now')",
    )
    .bind(tenant_id)
    .bind(max_bytes)
    .bind(max_volumes)
    .execute(pool)
    .await?;
    Ok(())
}

/// Result of a quota admission check.
pub enum QuotaCheck {
    Ok,
    /// The additional bytes would exceed the tenant's byte quota (limit, would_be_used).
    Bytes {
        limit: i64,
        would_be: i64,
    },
    /// One more volume would exceed the tenant's volume-count quota (limit, current_count).
    Count {
        limit: i64,
        current: i64,
    },
}

/// Check whether creating one volume of `add_bytes` is within the tenant's quota.
pub async fn check_admission(
    pool: &SqlitePool,
    tenant_id: &str,
    add_bytes: i64,
) -> Result<QuotaCheck> {
    let q = get_quota(pool, tenant_id).await?;
    if q.max_bytes > 0 && q.used_bytes + add_bytes > q.max_bytes {
        return Ok(QuotaCheck::Bytes {
            limit: q.max_bytes,
            would_be: q.used_bytes + add_bytes,
        });
    }
    if q.max_volumes > 0 && q.volume_count + 1 > q.max_volumes {
        return Ok(QuotaCheck::Count {
            limit: q.max_volumes,
            current: q.volume_count,
        });
    }
    Ok(QuotaCheck::Ok)
}
