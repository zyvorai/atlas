// Copyright (c) 2026 ZyvorAI Labs Private Limited. All rights reserved.
//! Resource-id helpers using the prefix scheme from PDF §10.1 (`bkd_`, `cls_`, `pool_`, ...).

use uuid::Uuid;

fn short() -> String {
    // 12 hex chars is enough entropy for human-readable resource ids.
    Uuid::new_v4().simple().to_string()[..12].to_string()
}

fn prefixed(prefix: &str) -> String {
    format!("{prefix}_{}", short())
}

pub fn backend_id() -> String {
    prefixed("bkd")
}
pub fn cluster_id() -> String {
    prefixed("cls")
}
pub fn pool_id() -> String {
    prefixed("pool")
}
pub fn volume_id() -> String {
    prefixed("vol")
}
pub fn snapshot_id() -> String {
    prefixed("snap")
}
pub fn bucket_id() -> String {
    prefixed("bkt")
}
pub fn policy_id() -> String {
    prefixed("pol")
}
pub fn job_id() -> String {
    prefixed("job")
}
pub fn binding_id() -> String {
    prefixed("bind")
}

/// Deterministic id derived from a stable natural key, so re-discovering the same backend
/// resource yields the same id (idempotent upserts). Uses a 64-bit FNV-1a hash — not cryptographic.
pub fn stable_id(prefix: &str, natural_key: &str) -> String {
    const OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
    const PRIME: u64 = 0x0000_0100_0000_01b3;
    let mut hash = OFFSET;
    for b in natural_key.as_bytes() {
        hash ^= *b as u64;
        hash = hash.wrapping_mul(PRIME);
    }
    format!("{prefix}_{hash:012x}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prefixes_and_stability() {
        assert!(volume_id().starts_with("vol_"));
        assert!(backend_id().starts_with("bkd_"));
        // stable_id is deterministic for the same key.
        assert_eq!(
            stable_id("cls", "ceph:fsid-abc"),
            stable_id("cls", "ceph:fsid-abc")
        );
        assert_ne!(
            stable_id("cls", "ceph:fsid-abc"),
            stable_id("cls", "ceph:fsid-xyz")
        );
    }
}
