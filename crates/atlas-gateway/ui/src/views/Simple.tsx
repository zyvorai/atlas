// Copyright (c) 2026 ZyvorAI Labs Private Limited. All rights reserved.
// Compact read/light-write Centers: Policies, Backends, Kubernetes, Cluster, Metrics.
import { useState } from "react";
import { Boxes, Database, Gauge, Server, ShieldCheck } from "lucide-react";
import { submit } from "../api/client";
import {
  useBackends, useCephMetrics, useClusters, useInvalidate, useNodes, useOsds, usePolicies,
  usePools, useStorageClasses,
} from "../api/hooks";
import { Badge, Button, GlassSection, PageHeader } from "../ui/kit";
import { Table } from "../ui/Table";
import { fmtBytes, healthKind } from "../lib/format";

export function Policies() {
  const { data } = usePolicies();
  return (
    <div>
      <PageHeader icon={ShieldCheck} title="Policies" subtitle="Built-in intent → placement catalog (atlas-policy)" />
      <GlassSection title="Intent catalog">
        <Table rows={data} rowKey={(p) => p.intent}
          cols={[
            { h: "Intent", f: (p) => <Badge kind="info">{p.intent}</Badge> },
            { h: "StorageClass", f: (p) => p.storage_class, mono: true },
            { h: "Access", f: (p) => p.access_mode },
            { h: "Volume mode", f: (p) => p.volume_mode },
            { h: "Description", f: (p) => <span className="text-muted-foreground">{p.description}</span> },
          ]} />
      </GlassSection>
    </div>
  );
}

export function Backends() {
  const { data } = useBackends();
  const inv = useInvalidate();
  return (
    <div>
      <PageHeader icon={Server} title="Backends" subtitle="Registered storage backends and discovery"
        actions={<Button onClick={() => submit("post", "/backends/bkd_ceph_lab/discover", null, "discovery triggered", () => inv("clusters"))}>Discover</Button>} />
      <GlassSection title="Backends">
        <Table rows={data} rowKey={(b) => b.id}
          cols={[
            { h: "ID", f: (b) => b.id, mono: true },
            { h: "Name", f: (b) => b.name },
            { h: "Type", f: (b) => b.backend_type },
            { h: "Mode", f: (b) => b.mode },
            { h: "Status", f: (b) => <Badge kind={b.status === "active" ? "success" : "neutral"}>{b.status}</Badge> },
          ]} />
      </GlassSection>
    </div>
  );
}

export function Kubernetes() {
  const { data: scs } = useStorageClasses();
  return (
    <div>
      <PageHeader icon={Boxes} title="Kubernetes" subtitle="Live StorageClasses from the cluster" />
      <GlassSection title={<>StorageClasses <Badge kind="neutral">{scs?.length || 0}</Badge></>}>
        <Table rows={scs} rowKey={(s) => s.name}
          cols={[
            { h: "Name", f: (s) => s.name, mono: true },
            { h: "Provisioner", f: (s) => s.provisioner, mono: true },
            { h: "Reclaim", f: (s) => s.reclaim_policy || "—" },
            { h: "Binding", f: (s) => s.volume_binding_mode || "—" },
            { h: "Expand", f: (s) => (s.allow_volume_expansion ? "yes" : "no") },
            { h: "Ceph", f: (s) => (s.is_ceph ? <Badge kind="info">ceph</Badge> : "—") },
          ]} />
      </GlassSection>
    </div>
  );
}

export function Cluster() {
  const { data: clusters } = useClusters();
  const { data: pools } = usePools();
  const { data: osds } = useOsds();
  const { data: nodes } = useNodes();
  return (
    <div>
      <PageHeader icon={Database} title="Cluster" subtitle="Ceph cluster health, nodes, pools and OSDs" />
      <div className="grid lg:grid-cols-2 gap-4">
        <GlassSection title="Clusters">
          <Table rows={clusters} rowKey={(c) => c.id}
            cols={[
              { h: "Name", f: (c) => c.name, mono: true },
              { h: "FSID", f: (c) => <span className="mono text-muted-foreground">{(c.native_fsid || "").slice(0, 12) || "—"}</span> },
              { h: "Health", f: (c) => <Badge kind={healthKind(c.health)} dot>{c.health}</Badge> },
              { h: "Raw", f: (c) => fmtBytes(c.raw_capacity_bytes) },
              { h: "Used", f: (c) => fmtBytes(c.used_capacity_bytes) },
            ]} />
        </GlassSection>
        <GlassSection title={<>Nodes <Badge kind="neutral">{nodes?.length || 0}</Badge></>}>
          <Table rows={nodes} rowKey={(n) => n.host} cols={[{ h: "Host", f: (n) => n.host, mono: true }]} />
        </GlassSection>
        <GlassSection title="Pools">
          <Table rows={pools} rowKey={(p) => p.id}
            cols={[
              { h: "Name", f: (p) => p.name, mono: true },
              { h: "Kind", f: (p) => p.kind },
              { h: "Used", f: (p) => fmtBytes(p.used_bytes) },
              { h: "Max", f: (p) => fmtBytes(p.max_bytes) },
            ]} />
        </GlassSection>
        <GlassSection title="OSDs">
          <Table rows={osds} rowKey={(o) => String(o.id)}
            cols={[
              { h: "OSD", f: (o) => `osd.${o.osd_num ?? o.id}`, mono: true },
              { h: "Host", f: (o) => o.host },
              { h: "Up", f: (o) => <Badge kind={o.up ? "success" : "danger"} dot>{o.up ? "up" : "down"}</Badge> },
              { h: "Used", f: (o) => fmtBytes(o.used_bytes) },
            ]} />
        </GlassSection>
      </div>
    </div>
  );
}

export function Metrics() {
  const [prefix, setPrefix] = useState("");
  const { data } = useCephMetrics(prefix || undefined);
  return (
    <div>
      <PageHeader icon={Gauge} title="Metrics" subtitle="Curated Ceph mgr Prometheus metrics (capacity, OSD latency, pool usage, client I/O, recovery)" />
      <div className="mb-3">
        <input className="field w-72" placeholder="Filter by prefix, e.g. ceph_pool_" value={prefix} onChange={(e) => setPrefix(e.target.value)} />
      </div>
      <GlassSection title={<>Metrics <Badge kind="neutral">{data?.length || 0}</Badge></>}>
        <Table rows={data} rowKey={(m, i) => m.name + i}
          cols={[
            { h: "Metric", f: (m) => m.name, mono: true },
            { h: "Labels", f: (m) => <span className="mono text-muted-foreground text-[11px]">{Object.entries(m.labels || {}).map(([k, v]) => `${k}=${v}`).join(" ")}</span> },
            { h: "Value", f: (m) => (Number.isInteger(m.value) ? m.value.toLocaleString() : m.value.toFixed(3)) },
          ]} />
      </GlassSection>
    </div>
  );
}
