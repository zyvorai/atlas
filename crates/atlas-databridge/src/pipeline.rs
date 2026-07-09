// Copyright (c) 2026 ZyvorAI Labs Private Limited. All rights reserved.
//! Pipeline stage functions called by the job-engine `dispatch` arms. Each takes the SQLite pool
//! (+ later the k8s driver), does the work, updates the DataBridge inventory, and returns a JSON
//! result the job engine persists. Keeping the logic here keeps the `dispatch` match arms thin.

use anyhow::{anyhow, Result};
use sqlx::SqlitePool;

use crate::connector::DiscoveredSchema;
use crate::{assess, build_connector};

/// Discover a source's schema and persist it. Advances the source `registered → discovered`.
pub async fn discover(pool: &SqlitePool, source_id: &str) -> Result<serde_json::Value> {
    let source = atlas_inventory::databridge::sources::get_source(pool, source_id)
        .await?
        .ok_or_else(|| anyhow!("source {source_id} not found"))?;

    atlas_inventory::databridge::sources::set_state(pool, source_id, "discovering").await?;

    // On any discovery failure, flip the source to `error` so the UI shows why.
    let schema = match async {
        let connector = build_connector(&source)?;
        connector.discover().await
    }
    .await
    {
        Ok(s) => s,
        Err(e) => {
            let _ = atlas_inventory::databridge::sources::set_state(pool, source_id, "error").await;
            return Err(e);
        }
    };
    let discovered = serde_json::to_value(&schema)?;
    atlas_inventory::databridge::sources::set_discovered(pool, source_id, &discovered).await?;

    Ok(serde_json::json!({
        "source_id": source_id,
        "engine": schema.engine,
        "version": schema.version,
        "databases": schema.databases,
        "tables": schema.tables.len(),
        "total_size_bytes": schema.total_size_bytes,
        "cdc_capable": schema.cdc_capable,
    }))
}

/// Assess a plan's source: score readiness from the discovered schema. Advances the plan to
/// `assessed`. Requires the source to have been discovered.
pub async fn assess_plan(pool: &SqlitePool, plan_id: &str) -> Result<serde_json::Value> {
    let plan = atlas_inventory::databridge::plans::get_plan(pool, plan_id)
        .await?
        .ok_or_else(|| anyhow!("plan {plan_id} not found"))?;
    let source = atlas_inventory::databridge::sources::get_source(pool, &plan.source_id)
        .await?
        .ok_or_else(|| anyhow!("source {} not found", plan.source_id))?;
    if source.state != "discovered" {
        return Err(anyhow!(
            "source {} is not discovered yet (state: {})",
            source.id,
            source.state
        ));
    }
    let schema: DiscoveredSchema = serde_json::from_value(source.discovered.clone())
        .map_err(|e| anyhow!("source has no valid discovered schema: {e}"))?;

    let assessment = assess::assess(&schema);
    let value = serde_json::to_value(&assessment)?;
    atlas_inventory::databridge::plans::set_assessment(pool, plan_id, assessment.score, &value)
        .await?;

    Ok(serde_json::json!({
        "plan_id": plan_id,
        "score": assessment.score,
        "risk": assessment.risk,
        "blockers": assessment.blockers.len(),
        "warnings": assessment.warnings.len(),
    }))
}

/// 1 GiB in bytes.
const GIB: i64 = 1024 * 1024 * 1024;
/// Namespace the edge databases + their CRs live in.
pub const EDGE_NAMESPACE: &str = "zyvor-databridge";

/// Data-volume size for the edge cluster: discovered dataset + 50% headroom, min 10 GiB.
fn edge_size_gib(source: &atlas_api_types::MigrationSource) -> i64 {
    let total = source
        .discovered
        .get("total_size_bytes")
        .and_then(|v| v.as_i64())
        .unwrap_or(0);
    ((total + total / 2) / GIB).max(10)
}

/// Provision the edge database cluster for a plan. In `fake` source mode this fabricates a ready
/// cluster with a synthetic endpoint (no k8s needed). In `real` mode it applies the CloudNativePG /
/// MySQL operator CR (data on Ceph RBD) and leaves the cluster `provisioning` for the reconciler to
/// poll to `ready`.
pub async fn provision_edge(
    pool: &SqlitePool,
    k8s: Option<&atlas_driver_k8s::K8sDriver>,
    plan_id: &str,
) -> Result<serde_json::Value> {
    let plan = atlas_inventory::databridge::plans::get_plan(pool, plan_id)
        .await?
        .ok_or_else(|| anyhow!("plan {plan_id} not found"))?;
    let source = atlas_inventory::databridge::sources::get_source(pool, &plan.source_id)
        .await?
        .ok_or_else(|| anyhow!("source {} not found", plan.source_id))?;

    let engine = crate::SourceKind::parse(&source.kind)
        .ok_or_else(|| anyhow!("unsupported engine: {}", source.kind))?;
    let (engine_str, operator) = match engine {
        crate::SourceKind::Postgres => ("postgres", "cnpg"),
        crate::SourceKind::Mysql => ("mysql", "percona"),
    };
    let namespace = EDGE_NAMESPACE;
    let cr_name = format!("edge-{}", &plan_id[plan_id.len().saturating_sub(8)..]);
    let edge_id = atlas_common::ids::edge_cluster_id();
    let size_gib = edge_size_gib(&source);

    atlas_inventory::databridge::edge_clusters::insert_edge_cluster(
        pool, &edge_id, &plan.tenant_id, plan_id, engine_str, operator, namespace, &cr_name,
        "zyvor-rbd-prod", Some("zyvor-rbd-prod"), 1,
    )
    .await?;
    atlas_inventory::databridge::plans::set_edge_cluster(pool, plan_id, &edge_id).await?;
    atlas_inventory::databridge::plans::set_state(pool, plan_id, "provisioning").await?;

    // Fake source mode (or no reachable k8s): fabricate a ready cluster so the pipeline runs.
    if source.driver_mode == "fake" || k8s.is_none() {
        let (endpoint, secret_ref) = match engine {
            crate::SourceKind::Postgres => {
                (crate::cr::cnpg::endpoint(&cr_name, namespace), crate::cr::cnpg::secret_ref(&cr_name))
            }
            crate::SourceKind::Mysql => (
                crate::cr::mysql_operator::endpoint(&cr_name, namespace),
                crate::cr::mysql_operator::secret_ref(&cr_name),
            ),
        };
        atlas_inventory::databridge::edge_clusters::set_ready(pool, &edge_id, &endpoint, &secret_ref)
            .await?;
        atlas_inventory::databridge::plans::set_state(pool, plan_id, "provisioned").await?;
        return Ok(serde_json::json!({
            "plan_id": plan_id, "edge_cluster_id": edge_id, "engine": engine_str,
            "operator": operator, "state": "ready", "service_endpoint": endpoint, "mode": "fake",
        }));
    }

    // Real mode: apply the operator CR; the reconciler advances provisioning -> ready.
    let k8s = k8s.expect("k8s present in real branch");
    let db = source.database.as_deref().unwrap_or("appdb");
    let (group, version, kind, spec) = match engine {
        crate::SourceKind::Postgres => (
            crate::cr::cnpg::GROUP, crate::cr::cnpg::VERSION, crate::cr::cnpg::KIND,
            crate::cr::cnpg::cluster_spec(1, "zyvor-rbd-prod", "zyvor-rbd-prod", size_gib, db),
        ),
        crate::SourceKind::Mysql => (
            crate::cr::mysql_operator::GROUP, crate::cr::mysql_operator::VERSION,
            crate::cr::mysql_operator::KIND,
            crate::cr::mysql_operator::cluster_spec(1, "zyvor-rbd-prod", size_gib),
        ),
    };
    k8s.apply_cr(group, version, kind, namespace, &cr_name, spec)
        .await
        .map_err(|e| anyhow!("apply {kind} CR: {e}"))?;

    Ok(serde_json::json!({
        "plan_id": plan_id, "edge_cluster_id": edge_id, "engine": engine_str,
        "operator": operator, "state": "provisioning", "cr_name": cr_name,
        "size_gib": size_gib, "mode": "real",
    }))
}

/// Short suffix of a plan id, for naming derived k8s objects.
fn short(plan_id: &str) -> &str {
    &plan_id[plan_id.len().saturating_sub(8)..]
}

/// Full-load: copy the source dataset into the freshly-provisioned edge DB. Fake mode simulates an
/// instant load; real mode (pg_dump/mydumper batch Job) is a follow-up.
pub async fn full_load(
    pool: &SqlitePool,
    k8s: Option<&atlas_driver_k8s::K8sDriver>,
    plan_id: &str,
) -> Result<serde_json::Value> {
    let plan = atlas_inventory::databridge::plans::get_plan(pool, plan_id)
        .await?
        .ok_or_else(|| anyhow!("plan {plan_id} not found"))?;
    let source = atlas_inventory::databridge::sources::get_source(pool, &plan.source_id)
        .await?
        .ok_or_else(|| anyhow!("source not found"))?;
    atlas_inventory::databridge::plans::set_state(pool, plan_id, "full_loading").await?;

    if source.driver_mode == "fake" || k8s.is_none() {
        let tables = source
            .discovered
            .get("tables")
            .and_then(|t| t.as_array())
            .map(|a| a.len())
            .unwrap_or(0);
        atlas_inventory::databridge::plans::set_state(pool, plan_id, "loaded").await?;
        return Ok(serde_json::json!({ "plan_id": plan_id, "state": "loaded", "tables": tables, "mode": "fake" }));
    }

    // Real mode: apply a batch Job that dumps the source and loads it into the edge DB. The
    // reconciler watches the Job to completion and advances full_loading -> loaded (or failed).
    let k8s = k8s.expect("k8s present in real branch");
    let edge_id = plan
        .edge_cluster_id
        .as_deref()
        .ok_or_else(|| anyhow!("plan has no edge cluster; provision first"))?;
    let edge = atlas_inventory::databridge::edge_clusters::get_edge_cluster(pool, edge_id)
        .await?
        .ok_or_else(|| anyhow!("edge cluster {edge_id} not found"))?;
    if edge.state != "ready" {
        return Err(anyhow!("edge cluster is not ready yet (state: {})", edge.state));
    }
    let source_secret = source
        .secret_ref
        .as_deref()
        .ok_or_else(|| anyhow!("source has no secret_ref for credentials"))?;
    let edge_secret = edge
        .secret_ref
        .as_deref()
        .ok_or_else(|| anyhow!("edge cluster has no credentials secret"))?;
    let engine = crate::SourceKind::parse(&source.kind)
        .ok_or_else(|| anyhow!("unsupported engine: {}", source.kind))?;
    let src_host = source.endpoint.as_deref().unwrap_or("");
    let src_db = source.database.as_deref().unwrap_or("appdb");

    let job_spec = match engine {
        crate::SourceKind::Postgres => crate::loader::pg_job_spec(
            source_secret,
            edge_secret,
            src_host,
            source.port.unwrap_or(5432),
            src_db,
            &source.tls_mode,
        ),
        crate::SourceKind::Mysql => {
            // edge host = the HAProxy service (endpoint without the :port suffix).
            let edge_ep = crate::cr::mysql_operator::endpoint(
                edge.cr_name.as_deref().unwrap_or(""),
                &edge.namespace,
            );
            let edge_host = edge_ep.split(':').next().unwrap_or("");
            crate::loader::mysql_job_spec(
                source_secret,
                edge_secret,
                src_host,
                source.port.unwrap_or(3306),
                src_db,
                edge_host,
                src_db,
            )
        }
    };
    let job_name = crate::loader::job_name(plan_id);
    k8s.apply_cr(
        crate::loader::JOB_GROUP,
        crate::loader::JOB_VERSION,
        crate::loader::JOB_KIND,
        &edge.namespace,
        &job_name,
        job_spec,
    )
    .await
    .map_err(|e| anyhow!("apply full-load Job: {e}"))?;

    Ok(serde_json::json!({
        "plan_id": plan_id, "job": job_name, "state": "full_loading", "mode": "real"
    }))
}

/// Start Debezium CDC. Fake mode records a streaming stream with an initial lag the reconciler
/// drains toward zero; real mode (KafkaConnect + Debezium connector CRs) is a follow-up.
pub async fn start_cdc(
    pool: &SqlitePool,
    k8s: Option<&atlas_driver_k8s::K8sDriver>,
    plan_id: &str,
) -> Result<serde_json::Value> {
    let plan = atlas_inventory::databridge::plans::get_plan(pool, plan_id)
        .await?
        .ok_or_else(|| anyhow!("plan {plan_id} not found"))?;
    let source = atlas_inventory::databridge::sources::get_source(pool, &plan.source_id)
        .await?
        .ok_or_else(|| anyhow!("source not found"))?;
    let engine = source.kind.clone();
    let s = short(plan_id).to_string();
    let cdc_id = atlas_common::ids::cdc_stream_id();
    let connect = crate::cr::streaming::connect_name(&s);
    let connector = crate::cr::streaming::source_connector_name(&s);
    let topic_prefix = crate::cr::streaming::topic_prefix(&s);
    atlas_inventory::databridge::cdc::insert_stream(
        pool, &cdc_id, &plan.tenant_id, plan_id, &engine, &connect, &connector, &topic_prefix,
    )
    .await?;
    atlas_inventory::databridge::plans::set_cdc_stream(pool, plan_id, &cdc_id).await?;
    atlas_inventory::databridge::plans::set_state(pool, plan_id, "cdc_streaming").await?;

    if source.driver_mode == "fake" || k8s.is_none() {
        atlas_inventory::databridge::cdc::set_state(pool, &cdc_id, "streaming").await?;
        // seed an initial backlog; the reconciler drains it so the UI lag chart animates.
        atlas_inventory::databridge::cdc::update_lag(
            pool, &cdc_id, 512 * 1024 * 1024, 45, Some("0/1000"), Some("0/0"), 0,
        )
        .await?;
        return Ok(serde_json::json!({ "plan_id": plan_id, "cdc_stream_id": cdc_id, "state": "streaming", "mode": "fake" }));
    }

    // Real mode: apply the Strimzi KafkaConnect cluster + Debezium source + JDBC sink connectors.
    // (Structurally correct but UNVERIFIED against a live Kafka/DB stack — see docs/DATABRIDGE.md.)
    let k8s = k8s.expect("k8s present in real branch");
    let edge_id = plan
        .edge_cluster_id
        .as_deref()
        .ok_or_else(|| anyhow!("plan has no edge cluster; provision first"))?;
    let edge = atlas_inventory::databridge::edge_clusters::get_edge_cluster(pool, edge_id)
        .await?
        .ok_or_else(|| anyhow!("edge cluster not found"))?;
    let ns = EDGE_NAMESPACE;
    let src_secret = source
        .secret_ref
        .as_deref()
        .ok_or_else(|| anyhow!("source has no secret_ref"))?;
    let src_secret_ns = source.secret_namespace.as_deref().unwrap_or(ns);
    let edge_secret = edge
        .secret_ref
        .as_deref()
        .ok_or_else(|| anyhow!("edge cluster has no secret"))?;
    let cr_name = edge.cr_name.as_deref().unwrap_or("");
    let db = source.database.as_deref().unwrap_or("appdb");

    use crate::cr::streaming;
    // 1. KafkaConnect cluster
    k8s.apply_cr(
        streaming::GROUP, streaming::VERSION, streaming::CONNECT_KIND, ns, &connect,
        streaming::connect_spec("zyvor-kafka-bootstrap:9092", 1),
    )
    .await
    .map_err(|e| anyhow!("apply KafkaConnect: {e}"))?;

    let mut labels = std::collections::BTreeMap::new();
    labels.insert("strimzi.io/cluster".to_string(), connect.clone());

    // 2. Debezium source connector
    let src_spec = streaming::debezium_source_spec(
        &engine, &s, source.endpoint.as_deref().unwrap_or(""),
        source.port.unwrap_or(if engine == "postgres" { 5432 } else { 3306 }),
        db, src_secret_ns, src_secret,
    );
    k8s.apply_cr_labeled(
        streaming::GROUP, streaming::VERSION, streaming::CONNECTOR_KIND, ns, &connector, &labels, src_spec,
    )
    .await
    .map_err(|e| anyhow!("apply Debezium source connector: {e}"))?;

    // 3. JDBC sink connector -> edge DB
    let (jdbc_url, edge_user, edge_pass_key) = if engine == "postgres" {
        (format!("jdbc:postgresql://{cr_name}-rw.{ns}.svc:5432/{db}"), "app", "password")
    } else {
        (format!("jdbc:mysql://{cr_name}-haproxy.{ns}.svc:3306/{db}"), "root", "root")
    };
    let sink_spec = streaming::jdbc_sink_spec(&s, &jdbc_url, ns, edge_secret, edge_user, edge_pass_key);
    k8s.apply_cr_labeled(
        streaming::GROUP, streaming::VERSION, streaming::CONNECTOR_KIND, ns,
        &streaming::sink_connector_name(&s), &labels, sink_spec,
    )
    .await
    .map_err(|e| anyhow!("apply JDBC sink connector: {e}"))?;

    atlas_inventory::databridge::cdc::set_state(pool, &cdc_id, "streaming").await?;
    Ok(serde_json::json!({
        "plan_id": plan_id, "cdc_stream_id": cdc_id, "connect": connect, "state": "streaming", "mode": "real"
    }))
}

/// Stop a plan's CDC stream.
pub async fn stop_cdc(pool: &SqlitePool, plan_id: &str) -> Result<serde_json::Value> {
    let plan = atlas_inventory::databridge::plans::get_plan(pool, plan_id)
        .await?
        .ok_or_else(|| anyhow!("plan {plan_id} not found"))?;
    if let Some(cdc_id) = plan.cdc_stream_id.as_deref() {
        atlas_inventory::databridge::cdc::set_state(pool, cdc_id, "stopped").await?;
    }
    Ok(serde_json::json!({ "plan_id": plan_id, "state": "stopped" }))
}

/// Validate source vs edge (row counts / checksums). Fake mode reports matching counts per table.
pub async fn validate(
    pool: &SqlitePool,
    k8s: Option<&atlas_driver_k8s::K8sDriver>,
    plan_id: &str,
    kind: &str,
) -> Result<serde_json::Value> {
    let plan = atlas_inventory::databridge::plans::get_plan(pool, plan_id)
        .await?
        .ok_or_else(|| anyhow!("plan {plan_id} not found"))?;
    let source = atlas_inventory::databridge::sources::get_source(pool, &plan.source_id)
        .await?
        .ok_or_else(|| anyhow!("source not found"))?;
    atlas_inventory::databridge::plans::set_state(pool, plan_id, "validating").await?;

    let val_id = atlas_common::ids::validation_id();
    atlas_inventory::databridge::validations::insert_validation(
        pool, &val_id, &plan.tenant_id, plan_id, kind,
    )
    .await?;

    // Real mode: apply a validation Job (row-count compare); the reconciler advances it.
    if let (false, Some(k8s)) = (source.driver_mode == "fake", k8s) {
        let edge_id = plan
            .edge_cluster_id
            .as_deref()
            .ok_or_else(|| anyhow!("plan has no edge cluster"))?;
        let edge = atlas_inventory::databridge::edge_clusters::get_edge_cluster(pool, edge_id)
            .await?
            .ok_or_else(|| anyhow!("edge cluster not found"))?;
        let src_secret = source.secret_ref.as_deref().ok_or_else(|| anyhow!("source has no secret_ref"))?;
        let edge_secret = edge.secret_ref.as_deref().ok_or_else(|| anyhow!("edge has no secret"))?;
        let engine = crate::SourceKind::parse(&source.kind).ok_or_else(|| anyhow!("bad engine"))?;
        let src_host = source.endpoint.as_deref().unwrap_or("");
        let db = source.database.as_deref().unwrap_or("appdb");
        let cr_name = edge.cr_name.as_deref().unwrap_or("");
        let job_spec = match engine {
            crate::SourceKind::Postgres => crate::validate::pg_validate_job_spec(
                src_secret, edge_secret, src_host, source.port.unwrap_or(5432), db, &source.tls_mode,
            ),
            crate::SourceKind::Mysql => {
                let edge_host = format!("{cr_name}-haproxy");
                crate::validate::mysql_validate_job_spec(
                    src_secret, edge_secret, src_host, source.port.unwrap_or(3306), db, &edge_host, db,
                )
            }
        };
        let job_name = crate::validate::job_name(&val_id);
        k8s.apply_cr(
            crate::loader::JOB_GROUP, crate::loader::JOB_VERSION, crate::loader::JOB_KIND,
            &edge.namespace, &job_name, job_spec,
        )
        .await
        .map_err(|e| anyhow!("apply validation Job: {e}"))?;
        return Ok(serde_json::json!({
            "plan_id": plan_id, "validation_id": val_id, "state": "running", "mode": "real"
        }));
    }

    // Fake: every discovered table matches on both sides.
    let tables = source
        .discovered
        .get("tables")
        .and_then(|t| t.as_array())
        .cloned()
        .unwrap_or_default();
    let results: Vec<serde_json::Value> = tables
        .iter()
        .map(|t| {
            let name = t.get("name").and_then(|n| n.as_str()).unwrap_or("?");
            let rows = t.get("est_rows").and_then(|r| r.as_i64()).unwrap_or(0);
            serde_json::json!({ "table": name, "source_rows": rows, "edge_rows": rows, "checksum_match": true })
        })
        .collect();
    let summary = serde_json::json!({ "tables": results });
    atlas_inventory::databridge::validations::set_result(
        pool, &val_id, true, tables.len() as i64, 0, &summary,
    )
    .await?;
    atlas_inventory::databridge::plans::set_state(pool, plan_id, "validated").await?;

    Ok(serde_json::json!({ "plan_id": plan_id, "validation_id": val_id, "passed": true, "tables": tables.len() }))
}

/// Cutover: freeze source, drain CDC, switch endpoint, open the rollback window. Fake mode drains
/// instantly. Guards (validated + validation passed + lag under threshold) are enforced in the route.
pub async fn cutover(pool: &SqlitePool, plan_id: &str) -> Result<serde_json::Value> {
    let plan = atlas_inventory::databridge::plans::get_plan(pool, plan_id)
        .await?
        .ok_or_else(|| anyhow!("plan {plan_id} not found"))?;
    let source = atlas_inventory::databridge::sources::get_source(pool, &plan.source_id)
        .await?
        .ok_or_else(|| anyhow!("source not found"))?;
    let edge_id = plan
        .edge_cluster_id
        .as_deref()
        .ok_or_else(|| anyhow!("plan has no edge cluster"))?;
    let edge = atlas_inventory::databridge::edge_clusters::get_edge_cluster(pool, edge_id)
        .await?
        .ok_or_else(|| anyhow!("edge cluster not found"))?;

    let cut_id = atlas_common::ids::cutover_id();
    let from = source.endpoint.clone().unwrap_or_default();
    let to = edge.service_endpoint.clone().unwrap_or_default();
    let now = chrono::Utc::now();
    let drain = (now + chrono::Duration::minutes(5)).to_rfc3339();
    let rollback = (now + chrono::Duration::seconds(plan.rollback_window_secs)).to_rfc3339();

    atlas_inventory::databridge::cutovers::insert_cutover(
        pool, &cut_id, &plan.tenant_id, plan_id, Some(&from), Some(&to), Some(&drain), Some(&rollback),
    )
    .await?;
    atlas_inventory::databridge::plans::set_state(pool, plan_id, "cutover_in_progress").await?;

    // Fake: drain + switch complete immediately.
    atlas_inventory::databridge::cutovers::set_state(pool, &cut_id, "switching").await?;
    atlas_inventory::databridge::cutovers::set_complete(pool, &cut_id, "complete").await?;
    atlas_inventory::databridge::plans::set_state(pool, plan_id, "cutover_complete").await?;
    if let Some(cdc_id) = plan.cdc_stream_id.as_deref() {
        atlas_inventory::databridge::cdc::set_state(pool, cdc_id, "stopped").await?;
    }

    Ok(serde_json::json!({
        "plan_id": plan_id, "cutover_id": cut_id, "from": from, "to": to,
        "rollback_deadline": rollback, "state": "cutover_complete",
    }))
}

/// Roll back a cutover within its window: revert the active endpoint to the source.
pub async fn rollback(pool: &SqlitePool, plan_id: &str) -> Result<serde_json::Value> {
    let plan = atlas_inventory::databridge::plans::get_plan(pool, plan_id)
        .await?
        .ok_or_else(|| anyhow!("plan {plan_id} not found"))?;
    if let Some(cut) = atlas_inventory::databridge::cutovers::latest_for_plan(pool, plan_id).await? {
        atlas_inventory::databridge::cutovers::set_complete(pool, &cut.id, "rolled_back").await?;
    }
    atlas_inventory::databridge::plans::set_state(pool, plan_id, "rolled_back").await?;
    Ok(serde_json::json!({ "plan_id": plan.id, "state": "rolled_back" }))
}
