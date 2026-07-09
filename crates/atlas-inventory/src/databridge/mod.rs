// Copyright (c) 2026 ZyvorAI Labs Private Limited. All rights reserved.
//! DataBridge inventory: cloud-to-edge database migration records.
//! One module per resource, each following the `buckets.rs` insert/set/get/list/row_to pattern.

pub mod cdc;
pub mod cutovers;
pub mod edge_clusters;
pub mod plans;
pub mod sources;
pub mod validations;

#[cfg(test)]
mod tests {
    use crate::{connect, migrate};

    #[tokio::test]
    async fn databridge_round_trip() {
        let pool = connect("sqlite::memory:").await.unwrap();
        migrate(&pool).await.unwrap();

        // source
        super::sources::insert_source(
            &pool, "src_test", "global", "prod-pg", "postgres", "rds",
            Some("prod.abc.rds.amazonaws.com"), Some(5432), Some("appdb"),
            Some("rds-secret"), Some("zyvor-databridge"), "require", "fake",
        ).await.unwrap();
        super::sources::set_discovered(&pool, "src_test", &serde_json::json!({"tables": 3})).await.unwrap();
        let s = super::sources::get_source(&pool, "src_test").await.unwrap().unwrap();
        assert_eq!(s.state, "discovered");
        assert_eq!(s.discovered["tables"], 3);
        assert_eq!(super::sources::list_sources(&pool).await.unwrap().len(), 1);

        // plan + state transitions
        super::plans::insert_plan(&pool, "mplan_test", "global", "prod migration", "src_test", 259200).await.unwrap();
        super::plans::set_assessment(&pool, "mplan_test", 82, &serde_json::json!({"blockers": []})).await.unwrap();
        let p = super::plans::get_plan(&pool, "mplan_test").await.unwrap().unwrap();
        assert_eq!(p.state, "assessed");
        assert_eq!(p.readiness_score, 82);

        // edge cluster + cdc lag + validation + cutover
        super::edge_clusters::insert_edge_cluster(&pool, "edb_test", "global", "mplan_test", "postgres", "cnpg", "zyvor-databridge", "pg-edge", "zyvor-rbd-prod", Some("zyvor-rbd-fast"), 1).await.unwrap();
        super::edge_clusters::set_ready(&pool, "edb_test", "pg-edge.zyvor-databridge:5432", "pg-edge-app").await.unwrap();
        assert_eq!(super::edge_clusters::list_by_state(&pool, "ready").await.unwrap().len(), 1);

        super::cdc::insert_stream(&pool, "cdc_test", "global", "mplan_test", "postgres", "kc", "dbz-pg", "src_test").await.unwrap();
        super::cdc::update_lag(&pool, "cdc_test", 1024, 3, Some("0/AB"), Some("0/AA"), 42).await.unwrap();
        let c = super::cdc::get_stream(&pool, "cdc_test").await.unwrap().unwrap();
        assert_eq!(c.lag_seconds, 3);
        assert_eq!(c.events_total, 42);

        super::validations::insert_validation(&pool, "val_test", "global", "mplan_test", "rowcount").await.unwrap();
        super::validations::set_result(&pool, "val_test", true, 3, 0, &serde_json::json!([])).await.unwrap();
        assert_eq!(super::validations::latest_for_plan(&pool, "mplan_test").await.unwrap().unwrap().state, "passed");

        super::cutovers::insert_cutover(&pool, "cut_test", "global", "mplan_test", Some("src"), Some("edge"), None, None).await.unwrap();
        super::cutovers::set_complete(&pool, "cut_test", "complete").await.unwrap();
        assert_eq!(super::cutovers::latest_for_plan(&pool, "mplan_test").await.unwrap().unwrap().state, "complete");
    }
}
