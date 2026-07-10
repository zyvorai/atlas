// Copyright (c) 2026 ZyvorAI Labs Private Limited. All rights reserved.
//! Token revocation deny-list (day-2 governance). A revoked JWT id (`jti`) is rejected by the auth
//! middleware, so a leaked or rotated service-account credential can be killed before its TTL.

use anyhow::Result;
use sqlx::{Row, SqlitePool};

/// Revoke a token by its `jti`. Idempotent. Self-prunes revocations older than 30 days (token TTLs
/// are capped well under that, so any such token has certainly expired and no longer needs listing).
pub async fn revoke(pool: &SqlitePool, jti: &str, by: &str) -> Result<()> {
    sqlx::query("INSERT OR IGNORE INTO revoked_tokens (jti, revoked_by) VALUES (?, ?)")
        .bind(jti)
        .bind(by)
        .execute(pool)
        .await?;
    sqlx::query(
        "DELETE FROM revoked_tokens WHERE revoked_at < strftime('%Y-%m-%dT%H:%M:%fZ','now','-30 days')",
    )
    .execute(pool)
    .await?;
    Ok(())
}

/// Whether a token id has been revoked. Empty `jti` (legacy tokens) is never revoked.
pub async fn is_revoked(pool: &SqlitePool, jti: &str) -> Result<bool> {
    if jti.is_empty() {
        return Ok(false);
    }
    let n: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM revoked_tokens WHERE jti = ?")
        .bind(jti)
        .fetch_one(pool)
        .await?;
    Ok(n > 0)
}

/// List the current revocations (newest first).
pub async fn list(pool: &SqlitePool) -> Result<Vec<serde_json::Value>> {
    let rows = sqlx::query("SELECT jti, revoked_by, revoked_at FROM revoked_tokens ORDER BY revoked_at DESC")
        .fetch_all(pool)
        .await?;
    Ok(rows
        .into_iter()
        .map(|r| {
            serde_json::json!({
                "jti": r.get::<String, _>("jti"),
                "revoked_by": r.get::<Option<String>, _>("revoked_by"),
                "revoked_at": r.get::<String, _>("revoked_at"),
            })
        })
        .collect())
}
