// Copyright (c) 2026 ZyvorAI Labs Private Limited. All rights reserved.
import { LayoutDashboard } from "lucide-react";
import { Area, AreaChart, ResponsiveContainer } from "recharts";
import { useAlerts, useClusters, useOsds, usePools, useSummary } from "../api/hooks";
import { Badge, Card, GlassSection, PageHeader, StatCard } from "../ui/kit";
import { Table } from "../ui/Table";
import { fmtBytes, healthKind, num, stateKind } from "../lib/format";

function Gauge({ pct }: { pct: number }) {
  return (
    <div className="h-2 rounded-full bg-white/5 overflow-hidden mt-3">
      <div className="h-full rounded-full bg-gradient-to-r from-sky-400 to-blue-600" style={{ width: `${Math.min(100, pct)}%` }} />
    </div>
  );
}

export default function Overview() {
  const { data: s } = useSummary();
  const { data: clusters } = useClusters();
  const { data: pools } = usePools();
  const { data: osds } = useOsds();
  const { data: alerts } = useAlerts("open");
  const io = s?.client_io;
  const rc = s?.recovery;
  // Synthetic sparkline seed from the read/write totals (visual only).
  const spark = io
    ? Array.from({ length: 16 }, (_, i) => ({ v: (io.read_ops_total % 1000) + Math.sin(i / 2) * 120 + i * 8 }))
    : [];

  return (
    <div>
      <PageHeader icon={LayoutDashboard} title="Command Deck" subtitle="Live storage health, capacity, and activity" />

      <div className="grid grid-cols-2 md:grid-cols-3 lg:grid-cols-6 gap-3 mb-4">
        <StatCard
          label="Capacity used"
          value={<>{fmtBytes(s?.used_capacity_bytes)} <span className="text-sm text-muted-foreground font-normal">/ {fmtBytes(s?.raw_capacity_bytes)}</span></>}
        >
          <Gauge pct={s?.used_capacity_percent || 0} />
        </StatCard>
        <StatCard label="Volumes" value={num(s?.volumes)} />
        <StatCard label="Snapshots" value={num(s?.snapshots)} />
        <StatCard label="Buckets / Backups" value={<>{num(s?.buckets)} <span className="text-sm text-muted-foreground font-normal">/ {num(s?.backups)}</span></>} />
        <StatCard label="Client I/O" value={<>{num(io?.read_ops_total)}<span className="text-sm text-muted-foreground font-normal">r</span> {num(io?.write_ops_total)}<span className="text-sm text-muted-foreground font-normal">w</span></>} sub={`${fmtBytes(io?.read_bytes_total)} read · ${fmtBytes(io?.write_bytes_total)} write`}>
          {spark.length > 0 && (
            <div className="h-8 -mx-1 mt-1">
              <ResponsiveContainer width="100%" height="100%">
                <AreaChart data={spark}>
                  <defs>
                    <linearGradient id="io" x1="0" y1="0" x2="0" y2="1">
                      <stop offset="0%" stopColor="#38BDF8" stopOpacity={0.5} />
                      <stop offset="100%" stopColor="#38BDF8" stopOpacity={0} />
                    </linearGradient>
                  </defs>
                  <Area type="monotone" dataKey="v" stroke="#38BDF8" strokeWidth={1.5} fill="url(#io)" isAnimationActive={false} />
                </AreaChart>
              </ResponsiveContainer>
            </div>
          )}
        </StatCard>
        <StatCard label="Recovery" value={`${num((rc?.pg_recovering || 0) + (rc?.pg_backfilling || 0))} PG`} sub={`${num(rc?.objects_degraded)} degraded · ${num(rc?.objects_unfound)} unfound`} />
      </div>

      <div className="grid lg:grid-cols-2 gap-4">
        <GlassSection title={<span className="flex items-center gap-2">Clusters <Badge kind="neutral">{clusters?.length || 0}</Badge></span>}>
          <Table
            cols={[
              { h: "Name", f: (c) => c.name, mono: true },
              { h: "Health", f: (c) => <Badge kind={healthKind(c.health)} dot>{c.health}</Badge> },
              { h: "Raw", f: (c) => fmtBytes(c.raw_capacity_bytes) },
              { h: "Used", f: (c) => fmtBytes(c.used_capacity_bytes) },
              { h: "Avail", f: (c) => fmtBytes(c.available_capacity_bytes) },
            ]}
            rows={clusters}
          />
        </GlassSection>

        <GlassSection title={<span className="flex items-center gap-2">Open alerts <Badge kind={alerts?.length ? "danger" : "success"}>{alerts?.length || 0}</Badge></span>}>
          <Table
            cols={[
              { h: "Severity", f: (a) => <Badge kind={stateKind(a.severity)}>{a.severity}</Badge> },
              { h: "Title", f: (a) => a.title },
              { h: "Resource", f: (a) => a.resource_id, mono: true },
            ]}
            rows={alerts}
            empty="No open alerts."
          />
        </GlassSection>
      </div>

      <div className="grid lg:grid-cols-2 gap-4 mt-4">
        <GlassSection title="Pools">
          <Table
            cols={[
              { h: "Name", f: (p) => p.name, mono: true },
              { h: "Kind", f: (p) => p.kind },
              { h: "Used", f: (p) => fmtBytes(p.used_bytes) },
              { h: "Max", f: (p) => fmtBytes(p.max_bytes) },
              { h: "Health", f: (p) => <Badge kind={healthKind(p.health)} dot>{p.health}</Badge> },
            ]}
            rows={pools}
          />
        </GlassSection>
        <GlassSection title="OSDs">
          <Table
            cols={[
              { h: "OSD", f: (o) => `osd.${o.osd_num ?? o.id}`, mono: true },
              { h: "Host", f: (o) => o.host },
              { h: "Up", f: (o) => <Badge kind={o.up ? "success" : "danger"} dot>{o.up ? "up" : "down"}</Badge> },
              { h: "In", f: (o) => (o.in_cluster ? "in" : "out") },
              { h: "Used", f: (o) => fmtBytes(o.used_bytes) },
              { h: "Cap", f: (o) => fmtBytes(o.capacity_bytes) },
            ]}
            rows={osds}
          />
        </GlassSection>
      </div>
    </div>
  );
}
