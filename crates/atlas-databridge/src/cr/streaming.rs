// Copyright (c) 2026 ZyvorAI Labs Private Limited. All rights reserved.
//! Strimzi + Debezium CR builders for CDC: a `KafkaConnect` cluster (with the Debezium + JDBC-sink
//! plugins and a Kubernetes Secret config-provider) plus two `KafkaConnector`s — a Debezium source
//! (reads the cloud DB's WAL/binlog into Kafka) and a JDBC sink (applies the topics to the edge DB).
//!
//! NOTE: these specs are structurally correct but UNVERIFIED against a live Strimzi/Kafka + real DB
//! stack (see docs/DATABRIDGE.md). Secret values use Strimzi's config-provider syntax
//! `${secrets:<ns>/<secret>:<key>}` so credentials never appear in the CR.

use serde_json::{json, Value};

pub const GROUP: &str = "kafka.strimzi.io";
pub const VERSION: &str = "v1beta2";
pub const CONNECT_KIND: &str = "KafkaConnect";
pub const CONNECTOR_KIND: &str = "KafkaConnector";

/// Names derived from a plan's short id, stable across reconciles.
pub fn connect_name(short: &str) -> String {
    format!("dbz-connect-{short}")
}
pub fn source_connector_name(short: &str) -> String {
    format!("dbz-src-{short}")
}
pub fn sink_connector_name(short: &str) -> String {
    format!("jdbc-sink-{short}")
}
pub fn topic_prefix(short: &str) -> String {
    format!("db{short}")
}

/// A `KafkaConnect` cluster spec with the Debezium + JDBC-sink plugins and a Secret config provider.
pub fn connect_spec(bootstrap_servers: &str, replicas: i64) -> Value {
    json!({
        "replicas": replicas.max(1),
        "bootstrapServers": bootstrap_servers,
        "config": {
            "group.id": "zyvor-databridge-connect",
            "offset.storage.topic": "zyvor-connect-offsets",
            "config.storage.topic": "zyvor-connect-configs",
            "status.storage.topic": "zyvor-connect-status",
            "config.providers": "secrets",
            "config.providers.secrets.class": "io.strimzi.kafka.KubernetesSecretConfigProvider"
        },
        // Plugins are baked into the Connect image in production; listed here for provenance.
        "build": { "plugins": ["debezium-postgres", "debezium-mysql", "jdbc-sink"] }
    })
}

/// Debezium source connector config for the given engine. `secret_ns`/`secret` reference the source
/// credentials Secret (keys `username`/`password`).
#[allow(clippy::too_many_arguments)]
pub fn debezium_source_spec(
    engine: &str,
    short: &str,
    host: &str,
    port: i64,
    database: &str,
    secret_ns: &str,
    secret: &str,
) -> Value {
    let user = format!("${{secrets:{secret_ns}/{secret}:username}}");
    let pass = format!("${{secrets:{secret_ns}/{secret}:password}}");
    let prefix = topic_prefix(short);
    let mut config = json!({
        "tasks.max": 1,
        "database.hostname": host,
        "database.port": port,
        "database.user": user,
        "database.password": pass,
        "database.dbname": database,
        "topic.prefix": prefix,
    });
    match engine {
        "postgres" => {
            config["connector.class"] = json!("io.debezium.connector.postgresql.PostgresConnector");
            config["plugin.name"] = json!("pgoutput");
            config["slot.name"] = json!(format!("dbz_{short}"));
            config["publication.autocreate.mode"] = json!("filtered");
        }
        _ => {
            config["connector.class"] = json!("io.debezium.connector.mysql.MySqlConnector");
            config["database.server.id"] = json!(184000 + (short.len() as i64));
            config["schema.history.internal.kafka.bootstrap.servers"] =
                json!("zyvor-kafka-bootstrap:9092");
            config["schema.history.internal.kafka.topic"] = json!(format!("dbz-history-{short}"));
        }
    }
    json!({ "class": config["connector.class"], "tasksMax": 1, "config": config })
}

/// JDBC sink connector config applying the source topics to the edge DB. `edge_secret` is the
/// operator app Secret (CNPG `uri`/Percona `root`); we build a JDBC URL to the edge service.
pub fn jdbc_sink_spec(short: &str, jdbc_url: &str, secret_ns: &str, edge_secret: &str, edge_user: &str, edge_pass_key: &str) -> Value {
    let prefix = topic_prefix(short);
    let config = json!({
        "connector.class": "io.aiven.connect.jdbc.JdbcSinkConnector",
        "tasks.max": 1,
        "topics.regex": format!("{prefix}\\..*"),
        "connection.url": jdbc_url,
        "connection.user": edge_user,
        "connection.password": format!("${{secrets:{secret_ns}/{edge_secret}:{edge_pass_key}}}"),
        "insert.mode": "upsert",
        "pk.mode": "record_key",
        "delete.enabled": true,
        "auto.create": true,
        "auto.evolve": true
    });
    json!({ "class": "io.aiven.connect.jdbc.JdbcSinkConnector", "tasksMax": 1, "config": config })
}

/// Whether a Strimzi `KafkaConnector` reports its connector `state == RUNNING`.
pub fn connector_running(status: &Value) -> bool {
    status
        .get("connectorStatus")
        .and_then(|c| c.get("connector"))
        .and_then(|c| c.get("state"))
        .and_then(|s| s.as_str())
        .map(|s| s.eq_ignore_ascii_case("RUNNING"))
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_are_stable() {
        assert_eq!(connect_name("abc123"), "dbz-connect-abc123");
        assert_eq!(source_connector_name("abc123"), "dbz-src-abc123");
        assert_eq!(topic_prefix("abc123"), "dbabc123");
    }

    #[test]
    fn pg_source_uses_pgoutput_and_secret_provider() {
        let s = debezium_source_spec("postgres", "abc123", "prod.rds.aws", 5432, "appdb", "zyvor-databridge", "src-creds");
        assert_eq!(s["config"]["connector.class"], "io.debezium.connector.postgresql.PostgresConnector");
        assert_eq!(s["config"]["plugin.name"], "pgoutput");
        assert_eq!(s["config"]["topic.prefix"], "dbabc123");
        assert_eq!(s["config"]["database.password"], "${secrets:zyvor-databridge/src-creds:password}");
    }

    #[test]
    fn mysql_source_sets_server_id_and_history() {
        let s = debezium_source_spec("mysql", "abc123", "prod.rds.aws", 3306, "appdb", "zyvor-databridge", "src-creds");
        assert_eq!(s["config"]["connector.class"], "io.debezium.connector.mysql.MySqlConnector");
        assert!(s["config"]["database.server.id"].as_i64().unwrap() > 0);
        assert!(s["config"]["schema.history.internal.kafka.topic"].as_str().unwrap().contains("abc123"));
    }

    #[test]
    fn sink_targets_topic_regex_and_upsert() {
        let s = jdbc_sink_spec("abc123", "jdbc:postgresql://edge-rw:5432/appdb", "zyvor-databridge", "edge-app", "app", "password");
        assert_eq!(s["config"]["insert.mode"], "upsert");
        assert_eq!(s["config"]["topics.regex"], "dbabc123\\..*");
    }

    #[test]
    fn running_status_parsed() {
        assert!(connector_running(&json!({ "connectorStatus": { "connector": { "state": "RUNNING" } } })));
        assert!(!connector_running(&json!({ "connectorStatus": { "connector": { "state": "FAILED" } } })));
        assert!(!connector_running(&json!({})));
    }
}
