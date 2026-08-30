// Copyright (c) 2026 ZyvorAI Labs Private Limited. All rights reserved.
/** In-console API surface map — curated from docs/API.md (not a full OpenAPI host). */
import { Link } from "react-router-dom";
import { PageHead } from "../ui/PageHead";

type Endpoint = { method: string; path: string; note: string };

const SECTIONS: { title: string; items: Endpoint[] }[] = [
  {
    title: "Meta",
    items: [
      { method: "GET", path: "/health", note: "Liveness-ish health" },
      { method: "GET", path: "/readyz", note: "Deep readiness (SQLite + driver)" },
      { method: "GET", path: "/version", note: "Gateway build identity" },
      { method: "GET", path: "/metrics", note: "Prometheus text (unauthenticated)" },
    ],
  },
  {
    title: "Auth & governance",
    items: [
      { method: "POST", path: "/api/atlas/v1/auth/login", note: "Console username/password → JWT" },
      { method: "POST", path: "/api/atlas/v1/auth/token", note: "Mint scoped service-account JWT" },
      { method: "GET", path: "/api/atlas/v1/auth/users", note: "List console users (admin)" },
      { method: "GET", path: "/api/atlas/v1/tenants", note: "Tenants, quotas, policies" },
    ],
  },
  {
    title: "Inventory",
    items: [
      { method: "GET", path: "/api/atlas/v1/clusters", note: "Storage clusters" },
      { method: "GET", path: "/api/atlas/v1/pools", note: "Pools (?kind=)" },
      { method: "GET", path: "/api/atlas/v1/volumes", note: "Volumes (?state=&kind=)" },
      { method: "GET", path: "/api/atlas/v1/snapshots", note: "Snapshots" },
      { method: "GET", path: "/api/atlas/v1/buckets", note: "RGW / object buckets" },
      { method: "GET", path: "/api/atlas/v1/osds", note: "OSD inventory" },
    ],
  },
  {
    title: "Write path (async jobs)",
    items: [
      { method: "POST", path: "/api/atlas/v1/volumes", note: "Create volume → job" },
      { method: "DELETE", path: "/api/atlas/v1/volumes/{id}", note: "Delete volume → job" },
      { method: "POST", path: "/api/atlas/v1/volumes/{id}/expand", note: "Expand → job" },
      { method: "POST", path: "/api/atlas/v1/snapshots", note: "Create snapshot → job" },
      { method: "GET", path: "/api/atlas/v1/jobs/{id}", note: "Job status" },
    ],
  },
  {
    title: "Observability",
    items: [
      { method: "GET", path: "/api/atlas/v1/metrics/summary", note: "Capacity + I/O + recovery rollup" },
      { method: "GET", path: "/api/atlas/v1/metrics/history", note: "Time series (?minutes=)" },
      { method: "GET", path: "/api/atlas/v1/events", note: "Unified activity feed" },
      { method: "GET", path: "/api/atlas/v1/alerts", note: "Alerts (?state=)" },
      { method: "GET", path: "/api/atlas/v1/ceph/status", note: "Native Ceph status JSON" },
    ],
  },
  {
    title: "DR (RBD mirroring)",
    items: [
      { method: "GET", path: "/api/atlas/v1/dr/status", note: "Peers / mirrors / RPO posture" },
      { method: "POST", path: "/api/atlas/v1/dr/peers", note: "Register peer cluster" },
      { method: "POST", path: "/api/atlas/v1/volumes/{id}/mirror", note: "Enable mirror (?mode=&peer=)" },
      { method: "POST", path: "/api/atlas/v1/dr/mirrors/{id}/promote", note: "Promote / failover" },
      { method: "POST", path: "/api/atlas/v1/dr/failover", note: "Confirm-gated failover runbook" },
    ],
  },
  {
    title: "DataBridge",
    items: [
      { method: "GET", path: "/api/atlas/v1/databridge/sources", note: "Cloud DB sources" },
      { method: "POST", path: "/api/atlas/v1/databridge/sources/{id}/discover", note: "Discover → job" },
      { method: "GET", path: "/api/atlas/v1/databridge/plans", note: "Migration plans" },
      { method: "POST", path: "/api/atlas/v1/databridge/plans/{id}/cdc/start", note: "Start Debezium CDC" },
      { method: "POST", path: "/api/atlas/v1/databridge/plans/{id}/cutover", note: "Cutover → job" },
    ],
  },
];

const GRPC: Endpoint[] = [
  { method: "RPC", path: "Health", note: "Liveness for the gRPC service" },
  { method: "RPC", path: "ListClusters", note: "Storage clusters" },
  { method: "RPC", path: "ListPools", note: "Pools" },
  { method: "RPC", path: "ListVolumes", note: "Volumes" },
  { method: "RPC", path: "GetVolume", note: "Volume detail" },
  { method: "RPC", path: "CreateVolume", note: "Create → job; Owner sets product_bindings" },
  { method: "RPC", path: "DeleteVolume", note: "Delete → job" },
  { method: "RPC", path: "ExpandVolume", note: "Expand → job" },
  { method: "RPC", path: "CreateSnapshot", note: "Create snapshot → job" },
  { method: "RPC", path: "ListSnapshots", note: "Snapshots" },
  { method: "RPC", path: "ListVolumesByOwner", note: "Volumes scoped to an owner" },
  { method: "RPC", path: "GetJob", note: "Job status" },
  { method: "RPC", path: "WatchJob", note: "Job status (stream)" },
  { method: "RPC", path: "ListAlerts", note: "Alerts" },
  { method: "RPC", path: "GetMetricsSummary", note: "Capacity + I/O + recovery rollup" },
  { method: "RPC", path: "ListBuckets", note: "RGW / object buckets" },
];

export default function ApiDocs() {
  return (
    <div className="at-stack">
      <PageHead
        eyebrow="CONSOLE · REFERENCE"
        title="API Docs"
        state="Curated Atlas REST + gRPC map for operators and product integrators. Full examples live in the repo docs/API.md."
      />

      <div className="at-panel">
        <div className="at-panel-bar">
          <span className="at-caption">Conventions</span>
        </div>
        <div className="at-docs-body">
          <p>
            Base path <code className="mono">/api/atlas/v1</code>. Errors:{" "}
            <code className="mono">{`{ "error": { "code", "message" } }`}</code>. Auth:{" "}
            <code className="mono">Authorization: Bearer &lt;JWT&gt;</code> when{" "}
            <code className="mono">ATLAS_AUTH_REQUIRED=1</code>.
          </p>
          <p>
            gRPC on <code className="mono">ATLAS_GRPC_ADDR</code> (default <code className="mono">:5111</code>,
            lab NodePort <strong>30512</strong>) — service <code className="mono">atlas.v1.AtlasStorage</code>.
            Product ownership: see <Link to="/access">Access</Link> tokens and repo{" "}
            <code className="mono">docs/PRODUCTS.md</code>.
          </p>
        </div>
      </div>

      <div className="at-panel">
        <div className="at-panel-bar">
          <span className="at-caption">gRPC · AtlasStorage</span>
        </div>
        <div className="at-docs-table">
          {GRPC.map((rpc) => (
            <div key={rpc.path} className="at-docs-row">
              <span className="at-docs-method">{rpc.method}</span>
              <code className="at-docs-path mono">{rpc.path}</code>
              <span className="at-docs-note">{rpc.note}</span>
            </div>
          ))}
        </div>
      </div>

      {SECTIONS.map((sec) => (
        <div key={sec.title} className="at-panel">
          <div className="at-panel-bar">
            <span className="at-caption">{sec.title}</span>
          </div>
          <div className="at-docs-table">
            {sec.items.map((ep) => (
              <div key={`${ep.method}-${ep.path}`} className="at-docs-row">
                <span className={`at-docs-method ${ep.method.toLowerCase()}`}>{ep.method}</span>
                <code className="at-docs-path mono">{ep.path}</code>
                <span className="at-docs-note">{ep.note}</span>
              </div>
            ))}
          </div>
        </div>
      ))}
    </div>
  );
}
