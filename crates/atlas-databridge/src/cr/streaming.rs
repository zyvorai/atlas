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
// Current Strimzi serves the Kafka/KafkaConnect/KafkaConnector kinds at v1 (v1beta2 was removed).
pub const VERSION: &str = "v1";
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

/// A `KafkaConnect` cluster spec (Strimzi v1). `groupId` + the three storage topics are top-level
/// required fields; the Secret config-provider lets connectors reference `${secrets:…}`.
///
/// `image` must be a Kafka Connect image that BUNDLES the Debezium (postgres/mysql) + a JDBC-sink
/// plugin (e.g. built with Strimzi's `spec.build` + a registry, or a prebuilt image). The default
/// Strimzi Connect image has no connector plugins, so leaving `image` empty means connectors won't
/// instantiate — set `ATLAS_DATABRIDGE_CONNECT_IMAGE` at deploy time.
pub fn connect_spec(bootstrap_servers: &str, replicas: i64, image: Option<&str>) -> Value {
    let mut spec = json!({
        "replicas": replicas.max(1),
        "bootstrapServers": bootstrap_servers,
        "groupId": "zyvor-databridge-connect",
        "configStorageTopic": "zyvor-connect-configs",
        "offsetStorageTopic": "zyvor-connect-offsets",
        "statusStorageTopic": "zyvor-connect-status",
        "config": {
            "config.providers": "secrets",
            "config.providers.secrets.class": "io.strimzi.kafka.KubernetesSecretConfigProvider",
            // Internal topics default to RF 3; a single-broker lab Kafka needs 1 or Connect can't
            // create them (/health 500 -> crashloop).
            "config.storage.replication.factor": 1,
            "offset.storage.replication.factor": 1,
            "status.storage.replication.factor": 1
        }
    });
    if let Some(img) = image.filter(|s| !s.is_empty()) {
        spec["image"] = json!(img);
    }
    spec
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
        // Encode Postgres `numeric` as a double, not a VariableScaleDecimal STRUCT the JDBC sink
        // can't bind. `never` skips the initial snapshot — the full-load already seeded the edge, so
        // CDC only needs to stream changes from here.
        "decimal.handling.mode": "double",
        "snapshot.mode": "never",
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
                json!("zyvor-kafka-kafka-bootstrap:9092");
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
        "topics.regex": format!("{prefix}[.][^.]+[.].*"),
        "connection.url": jdbc_url,
        "connection.user": edge_user,
        "connection.password": format!("${{secrets:{secret_ns}/{edge_secret}:{edge_pass_key}}}"),
        "insert.mode": "upsert",
        // The edge tables already exist (from full-load); PK column is assumed `id`.
        "pk.mode": "record_key",
        "pk.fields": "id",
        "auto.create": false,
        "auto.evolve": true,
        "consumer.override.auto.offset.reset": "earliest",
        // Flatten the Debezium envelope to the row image, and route `<prefix>.<schema>.<table>` topics
        // to the bare `<table>` name so the sink writes to the matching edge table.
        "transforms": "unwrap,route",
        "transforms.unwrap.type": "io.debezium.transforms.ExtractNewRecordState",
        "transforms.route.type": "org.apache.kafka.connect.transforms.RegexRouter",
        "transforms.route.regex": format!("{prefix}[.][^.]+[.](.*)"),
        "transforms.route.replacement": "$1"
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
        assert_eq!(s["config"]["pk.fields"], "id");
        assert_eq!(s["config"]["transforms.unwrap.type"], "io.debezium.transforms.ExtractNewRecordState");
        assert_eq!(s["config"]["topics.regex"], "dbabc123[.][^.]+[.].*");
    }

    #[test]
    fn running_status_parsed() {
        assert!(connector_running(&json!({ "connectorStatus": { "connector": { "state": "RUNNING" } } })));
        assert!(!connector_running(&json!({ "connectorStatus": { "connector": { "state": "FAILED" } } })));
        assert!(!connector_running(&json!({})));
    }
}
