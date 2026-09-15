// Copyright (c) 2026 ZyvorAI Labs Private Limited.
// SPDX-License-Identifier: AGPL-3.0-only OR LicenseRef-Atlas-Commercial
//! MCP (Model Context Protocol) server: exposes a small, read-only/advisory slice of Atlas's
//! inventory + Ops Advisor as MCP tools, so an MCP host (Hermes Agent, Claude, etc.) can operate
//! Atlas. Mounted at `/api/atlas/v1/mcp`, behind the same `auth_middleware` bearer-JWT gate as the
//! rest of the API — see `docs/HERMES_AGENT.md` for how to point a client at it.
//!
//! Deliberately no write/action tools in this first slice: mirrors the Ops Advisor's own
//! `can_execute: false` safety posture (`docs/AI_ADVISOR.md`) rather than letting an agent mutate
//! storage from day one.

use rmcp::{
    handler::server::{
        router::tool::ToolRouter,
        wrapper::{Json as ToolJson, Parameters},
    },
    model::{ErrorData as McpError, ServerCapabilities, ServerInfo},
    service::{RequestContext, RoleServer},
    tool, tool_handler, tool_router,
    transport::streamable_http_server::{
        session::local::LocalSessionManager, StreamableHttpServerConfig, StreamableHttpService,
    },
    ServerHandler,
};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::{
    auth::{tenant_scope, Actor},
    routes::ai::{compute_advisor, AdvisorMode},
    state::AppState,
};

fn internal_error(err: impl std::fmt::Display) -> McpError {
    McpError::internal_error(err.to_string(), None)
}

/// The `Actor` set by `auth_middleware` on the originating HTTP request, recovered from the MCP
/// request's `http::request::Parts` (rmcp forwards these into `RequestContext::extensions`, the
/// same mechanism `Extension<Actor>` extraction relies on for every REST handler).
fn actor_from(context: &RequestContext<RoleServer>) -> Result<Actor, McpError> {
    context
        .extensions
        .get::<axum::http::request::Parts>()
        .and_then(|parts| parts.extensions.get::<Actor>())
        .cloned()
        .ok_or_else(|| McpError::internal_error("no authenticated actor on this MCP session", None))
}

#[derive(Clone)]
pub struct AtlasMcp {
    state: AppState,
    // Read by the #[tool_handler] macro's generated `call_tool`/`list_tools` dispatch, not
    // directly by our own code — rustc's dead-code analysis doesn't see through that, matching
    // the upstream rmcp examples (examples/servers/src/common/counter.rs), which suppress the
    // same warning the same way.
    #[allow(dead_code)]
    tool_router: ToolRouter<AtlasMcp>,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct ClusterHealthArgs {
    /// Cluster id, as returned by `list_clusters`.
    id: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct ListPoolsArgs {
    /// Optional owning backend id filter.
    #[serde(default)]
    backend_id: Option<String>,
    /// Optional pool kind filter (rbd|cephfs_data|cephfs_metadata|rgw|nfs_export|other).
    #[serde(default)]
    kind: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct ListVolumesArgs {
    /// Optional volume state filter.
    #[serde(default)]
    state: Option<String>,
    /// Optional owning backend id filter.
    #[serde(default)]
    backend_id: Option<String>,
    /// Optional volume kind filter (block|filesystem|object).
    #[serde(default)]
    kind: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct ListAlertsArgs {
    /// Filter by alert state ("open" or "resolved"); omit for all.
    #[serde(default)]
    state: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct OpsAdvisorArgs {
    /// The operational question to ask the advisor.
    #[serde(default)]
    question: String,
}

#[tool_router]
impl AtlasMcp {
    pub fn new(state: AppState) -> Self {
        Self {
            state,
            tool_router: Self::tool_router(),
        }
    }

    #[tool(description = "List storage clusters known to Atlas, with capacity and health.")]
    async fn list_clusters(&self) -> Result<ToolJson<Value>, McpError> {
        let clusters = atlas_inventory::list_clusters(&self.state.pool)
            .await
            .map_err(internal_error)?;
        Ok(ToolJson(json!(clusters)))
    }

    #[tool(description = "Get the detailed health/capacity status of one cluster by id.")]
    async fn cluster_health(
        &self,
        Parameters(args): Parameters<ClusterHealthArgs>,
    ) -> Result<ToolJson<Value>, McpError> {
        let health = atlas_inventory::cluster_health(&self.state.pool, &args.id)
            .await
            .map_err(internal_error)?
            .ok_or_else(|| McpError::invalid_params(format!("unknown cluster {}", args.id), None))?;
        Ok(ToolJson(json!(health)))
    }

    #[tool(description = "List registered storage backends with a capacity/health summary.")]
    async fn list_backends(&self) -> Result<ToolJson<Value>, McpError> {
        let summary = atlas_inventory::backend_breakdown(&self.state.pool)
            .await
            .map_err(internal_error)?;
        Ok(ToolJson(json!(summary)))
    }

    #[tool(description = "List storage pools, optionally filtered by backend or kind.")]
    async fn list_pools(
        &self,
        Parameters(args): Parameters<ListPoolsArgs>,
    ) -> Result<ToolJson<Value>, McpError> {
        let pools = atlas_inventory::list_pools_filtered(
            &self.state.pool,
            args.backend_id.as_deref(),
            args.kind.as_deref(),
        )
        .await
        .map_err(internal_error)?;
        Ok(ToolJson(json!(pools)))
    }

    #[tool(
        description = "List storage volumes, optionally filtered by state/backend/kind. Scoped \
                        to the caller's own tenant unless the caller has the admin role."
    )]
    async fn list_volumes(
        &self,
        Parameters(args): Parameters<ListVolumesArgs>,
        context: RequestContext<RoleServer>,
    ) -> Result<ToolJson<Value>, McpError> {
        let actor = actor_from(&context)?;
        let tenant = tenant_scope(self.state.config.auth_required, &actor);
        let volumes = atlas_inventory::list_volumes_filtered(
            &self.state.pool,
            args.state.as_deref(),
            tenant,
            args.backend_id.as_deref(),
            args.kind.as_deref(),
        )
        .await
        .map_err(internal_error)?;
        Ok(ToolJson(json!(volumes)))
    }

    #[tool(description = "List alerts raised by the monitor worker, optionally filtered by state.")]
    async fn list_alerts(
        &self,
        Parameters(args): Parameters<ListAlertsArgs>,
    ) -> Result<ToolJson<Value>, McpError> {
        let alerts = atlas_inventory::alerts::list(&self.state.pool, args.state.as_deref())
            .await
            .map_err(internal_error)?;
        Ok(ToolJson(json!(alerts)))
    }

    #[tool(description = "Aggregate capacity/pool/volume metrics across every cluster.")]
    async fn metrics_summary(&self) -> Result<ToolJson<Value>, McpError> {
        let summary = atlas_inventory::metrics_summary(&self.state.pool)
            .await
            .map_err(internal_error)?;
        Ok(ToolJson(summary))
    }

    #[tool(
        description = "Run the Ops Advisor: an explainable storage-posture risk assessment with a \
                        prioritized, read-only runbook. Never executes any action (advisory only). \
                        Requires an operator-or-higher token."
    )]
    async fn ops_advisor(
        &self,
        Parameters(args): Parameters<OpsAdvisorArgs>,
        context: RequestContext<RoleServer>,
    ) -> Result<ToolJson<Value>, McpError> {
        let actor = actor_from(&context)?;
        // Local mode only: an agent-triggered call must never silently fan out to an external LLM
        // provider just because ATLAS_AI_BASE_URL happens to be configured.
        let result = compute_advisor(&self.state, &actor, &args.question, AdvisorMode::Local)
            .await
            .map_err(internal_error)?;
        Ok(ToolJson(json!(result)))
    }
}

#[tool_handler]
impl ServerHandler for AtlasMcp {
    fn get_info(&self) -> ServerInfo {
        let mut info = ServerInfo::default();
        info.capabilities = ServerCapabilities::builder().enable_tools().build();
        info.instructions = Some(
            "Atlas storage control plane — read-only inventory and advisory tools. No tool here \
             mutates storage; use the REST/gRPC API (docs/API.md) for write operations."
                .into(),
        );
        info
    }
}

/// Builds the nested MCP service for `routes::router()` to mount at `/mcp` inside the existing,
/// already-`auth_middleware`-gated `/api/atlas/v1` router.
pub(crate) fn router(state: AppState) -> StreamableHttpService<AtlasMcp, LocalSessionManager> {
    StreamableHttpService::new(
        move || Ok(AtlasMcp::new(state.clone())),
        LocalSessionManager::default().into(),
        StreamableHttpServerConfig::default(),
    )
}
