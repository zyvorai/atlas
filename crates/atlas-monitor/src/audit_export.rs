// Copyright (c) 2026 ZyvorAI Labs Private Limited.
// SPDX-License-Identifier: AGPL-3.0-only OR LicenseRef-Atlas-Commercial
//! Audit-log export to an external sink (SIEM webhook / generic HTTP collector) before retention
//! pruning deletes rows. Without this, `storage_audit_logs` retention is silent data loss with no
//! external record — exactly what a compliance review (e.g. APRA CPS 234) flags first. Mirrors
//! `notify::dispatch`'s webhook pattern.

use anyhow::Result;
use sqlx::SqlitePool;

/// Export audit rows older than `keep_days` to `url` as one batched JSON POST, then delete only
/// the rows that were actually exported. If the sink is unreachable, nothing is deleted — rows
/// stay in SQLite and are retried on the next tick, so a SIEM outage can never silently lose
/// audit history the way an unconditional `prune()` would.
pub async fn export_and_prune(pool: &SqlitePool, keep_days: i64, url: &str) -> Result<u64> {
    let rows = atlas_inventory::audit::rows_older_than(pool, keep_days).await?;
    if rows.is_empty() {
        return Ok(0);
    }
    let client = reqwest::Client::new();
    let resp = client
        .post(url)
        .json(&serde_json::json!({ "audit_logs": rows }))
        .timeout(std::time::Duration::from_secs(30))
        .send()
        .await?;
    if !resp.status().is_success() {
        anyhow::bail!("audit export webhook returned HTTP {}", resp.status());
    }
    let ids: Vec<i64> = rows
        .iter()
        .filter_map(|r| r.get("id").and_then(|v| v.as_i64()))
        .collect();
    atlas_inventory::audit::delete_ids(pool, &ids).await
}
