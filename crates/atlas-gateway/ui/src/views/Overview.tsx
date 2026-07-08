// Copyright (c) 2026 ZyvorAI Labs Private Limited. All rights reserved.
import { useEffect, useState } from "react";
import { AlertTriangle, LayoutDashboard } from "lucide-react";
import { Area, AreaChart, CartesianGrid, Line, LineChart, ResponsiveContainer, Tooltip, XAxis, YAxis } from "recharts";
import { useAlerts, useClusters, useForecast, useHistory, useOsds, usePools, useSummary } from "../api/hooks";
import { history, onHistory, recordSummary, seed } from "../store/history";
import { Badge, Card, GlassSection, PageHeader, RadialGauge, StatCard } from "../ui/kit";
import { Table } from "../ui/Table";
import { fmtBytes, healthKind, num, stateKind } from "../lib/format";

export default function Overview() {
  const { data: s } = useSummary();
  const { data: clusters } = useClusters();
  const { data: pools } = usePools();
  const { data: osds } = useOsds();
  const { data: alerts } = useAlerts("open");
  const { data: forecast } = useForecast();
  const io = s?.client_io;
  const rc = s?.recovery;
  const recovering = (rc?.pg_recovering || 0) + (rc?.pg_backfilling || 0);
  const [, setTick] = useState(0);
  const { data: serverHist } = useHistory();
  useEffect(() => onHistory(() => setTick((t) => t + 1)), []);
  useEffect(() => { if (serverHist) seed(serverHist); }, [serverHist]);
  useEffect(() => { if (s) recordSummary(s); }, [s]);
  const hist = history().map((h) => ({ ...h, ts: new Date(h.t).toLocaleTimeString([], { minute: "2-digit", second: "2-digit" }) }));
  // Synthetic sparkline seed from the read/write totals (visual only).
  const spark = io
    ? Array.from({ length: 16 }, (_, i) => ({ v: (io.read_ops_total % 1000) + Math.sin(i / 2) * 120 + i * 8 }))
    : [];

  return (
    <div>
      <PageHeader icon={LayoutDashboard} title="Command Deck" subtitle="Live storage health, capacity, and activity" />

      {recovering > 0 && (
        <div className="glass-card p-3 mb-4 flex items-center gap-3 border-l-2" style={{ borderLeftColor: "#FBBF24" }}>
          <AlertTriangle size={18} className="text-warning" />
          <span className="text-sm">Recovery in progress — <b>{num(recovering)}</b> PG(s), {num(rc?.objects_degraded)} degraded objects.</span>
        </div>
      )}

      <div className="grid lg:grid-cols-4 gap-3 mb-4">
        <Card hoverable className="p-4 flex items-center gap-4 lg:col-span-1">
          <RadialGauge pct={s?.used_capacity_percent || 0} label="used" />
          <div className="min-w-0">
            <div className="section-label">Capacity</div>
            <div className="text-lg font-bold mt-1">{fmtBytes(s?.used_capacity_bytes)}</div>
            <div className="text-xs text-muted-foreground">of {fmtBytes(s?.raw_capacity_bytes)} · {fmtBytes(s?.available_capacity_bytes)} free</div>
            {forecast && (forecast.samples || 0) >= 2 && (
              forecast.days_to_full != null ? (
                <div className={`text-xs mt-1 ${forecast.days_to_full <= 3 ? "text-danger" : forecast.days_to_full <= 14 ? "text-warning" : "text-muted-foreground"}`}>
                  Full in ~{forecast.days_to_full}d · +{fmtBytes(forecast.growth_bytes_per_day)}/day
                </div>
              ) : (
                <div className="text-xs mt-1 text-muted-foreground">Usage steady — no fill projection</div>
              )
            )}
          </div>
        </Card>
        <StatCard label="Client I/O" value={<>{num(io?.read_ops_total)}<span className="text-sm text-muted-foreground font-normal">r</span> {num(io?.write_ops_total)}<span className="text-sm text-muted-foreground font-normal">w</span></>} sub={`${fmtBytes(io?.read_bytes_total)} read · ${fmtBytes(io?.write_bytes_total)} write`}>
          {spark.length > 0 && (
            <div className="h-9 -mx-1 mt-1">
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
        <div className="grid grid-cols-2 gap-3 lg:col-span-2">
          <StatCard label="Volumes" value={num(s?.volumes)} />
          <StatCard label="Snapshots" value={num(s?.snapshots)} />
          <StatCard label="Buckets / Backups" value={<>{num(s?.buckets)} <span className="text-sm text-muted-foreground font-normal">/ {num(s?.backups)}</span></>} />
          <StatCard label="Recovery" value={`${num(recovering)} PG`} sub={`${num(rc?.objects_degraded)} degraded · ${num(rc?.objects_unfound)} unfound`} />
        </div>
      </div>

      <GlassSection title="Pool utilization" className="mb-4">
        <div className="p-4 space-y-3">
          {(pools || []).map((p) => {
            const used = p.used_bytes || 0;
            const max = (p.max_bytes || 0) + used;
            const pct = max > 0 ? (used / max) * 100 : 0;
            const color = pct >= 85 ? "#E23B3B" : pct >= 75 ? "#FBBF24" : "#38BDF8";
            return (
              <div key={p.id}>
                <div className="flex items-center gap-2 text-xs mb-1">
                  <span className="mono flex-1">{p.name}</span>
                  <span className="text-muted-foreground">{fmtBytes(used)} / {fmtBytes(max)}</span>
                  <span className="tabular-nums w-10 text-right" style={{ color }}>{pct.toFixed(0)}%</span>
                </div>
                <div className="h-1.5 rounded-full bg-white/5 overflow-hidden">
                  <div className="h-full rounded-full" style={{ width: `${Math.min(100, pct)}%`, background: color }} />
                </div>
              </div>
            );
          })}
          {pools && !pools.length && <div className="text-sm text-muted-foreground">No pools.</div>}
        </div>
      </GlassSection>

      {hist.length > 2 && (
        <div className="grid lg:grid-cols-2 gap-4 mb-4">
          <GlassSection title="Capacity trend (session)">
            <div className="p-3 h-40">
              <ResponsiveContainer width="100%" height="100%">
                <AreaChart data={hist} margin={{ top: 6, right: 8, left: -18, bottom: 0 }}>
                  <defs>
                    <linearGradient id="cap" x1="0" y1="0" x2="0" y2="1">
                      <stop offset="0%" stopColor="#38BDF8" stopOpacity={0.4} />
                      <stop offset="100%" stopColor="#38BDF8" stopOpacity={0} />
                    </linearGradient>
                  </defs>
                  <CartesianGrid strokeDasharray="3 3" stroke="rgba(255,255,255,0.05)" />
                  <XAxis dataKey="ts" tick={{ fontSize: 10, fill: "#8aa0bd" }} minTickGap={40} />
                  <YAxis domain={[0, 100]} tick={{ fontSize: 10, fill: "#8aa0bd" }} width={34} unit="%" />
                  <Tooltip contentStyle={{ background: "#0b1220", border: "1px solid #1f2a3a", borderRadius: 8, fontSize: 12 }} />
                  <Area type="monotone" dataKey="usedPct" name="used %" stroke="#38BDF8" strokeWidth={1.6} fill="url(#cap)" isAnimationActive={false} />
                </AreaChart>
              </ResponsiveContainer>
            </div>
          </GlassSection>
          <GlassSection title="Client IOPS (session)">
            <div className="p-3 h-40">
              <ResponsiveContainer width="100%" height="100%">
                <LineChart data={hist} margin={{ top: 6, right: 8, left: -18, bottom: 0 }}>
                  <CartesianGrid strokeDasharray="3 3" stroke="rgba(255,255,255,0.05)" />
                  <XAxis dataKey="ts" tick={{ fontSize: 10, fill: "#8aa0bd" }} minTickGap={40} />
                  <YAxis tick={{ fontSize: 10, fill: "#8aa0bd" }} width={34} />
                  <Tooltip contentStyle={{ background: "#0b1220", border: "1px solid #1f2a3a", borderRadius: 8, fontSize: 12 }} />
                  <Line type="monotone" dataKey="readIops" name="read" stroke="#38BDF8" strokeWidth={1.6} dot={false} isAnimationActive={false} />
                  <Line type="monotone" dataKey="writeIops" name="write" stroke="#a78bfa" strokeWidth={1.6} dot={false} isAnimationActive={false} />
                </LineChart>
              </ResponsiveContainer>
            </div>
          </GlassSection>
        </div>
      )}

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
