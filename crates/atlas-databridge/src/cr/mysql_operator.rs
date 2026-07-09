// Copyright (c) 2026 ZyvorAI Labs Private Limited. All rights reserved.
//! Percona XtraDB Cluster CR builder for the MySQL edge target. Data lands on a Ceph RBD
//! StorageClass; binlog is enabled by the operator image so Debezium can read it for CDC.

pub const GROUP: &str = "pxc.percona.com";
pub const VERSION: &str = "v1";
pub const KIND: &str = "PerconaXtraDBCluster";

/// Build the `spec` for a PerconaXtraDBCluster with the PXC data volume on `storage_class`.
pub fn cluster_spec(instances: i64, storage_class: &str, size_gib: i64) -> serde_json::Value {
    serde_json::json!({
        "crVersion": "1.14.0",
        "pxc": {
            "size": instances.max(1),
            "image": "percona/percona-xtradb-cluster:8.0",
            "volumeSpec": {
                "persistentVolumeClaim": {
                    "storageClassName": storage_class,
                    "resources": { "requests": { "storage": format!("{size_gib}Gi") } }
                }
            }
        },
        "haproxy": {
            "enabled": true,
            "size": 1,
            "image": "percona/percona-xtradb-cluster-operator:1.14.0-haproxy"
        }
    })
}

/// Percona reports readiness via `status.state == "ready"`.
pub fn is_ready(status: &serde_json::Value) -> bool {
    status
        .get("state")
        .and_then(|s| s.as_str())
        .map(|s| s.eq_ignore_ascii_case("ready"))
        .unwrap_or(false)
}

pub fn endpoint(cr_name: &str, namespace: &str) -> String {
    format!("{cr_name}-haproxy.{namespace}.svc:3306")
}
pub fn secret_ref(cr_name: &str) -> String {
    format!("{cr_name}-secrets")
}
