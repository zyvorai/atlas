// Copyright (c) 2026 ZyvorAI Labs Private Limited. All rights reserved.
// Ceph — live cluster introspection: health checks, quorum/daemons, PG states, client I/O,
// the CRUSH OSD tree, and per-pool df. Backed by /ceph/status, /ceph/osd-tree, /ceph/df.
import { Aperture, ChevronRight } from "lucide-react";
import { useCephDf, useCephOsdDf, useCephOsdTree, useCephStatus } from "../api/hooks";
import { Badge, GlassSection, PageHeader, StatCard } from "../ui/kit";
import { Table } from "../ui/Table";
import { fmtBytes, num } from "../lib/format";

const healthKind = (s?: string) => (s === "HEALTH_OK" ? "success" : s === "HEALTH_ERR" ? "danger" : "warning");
const PG_COLOR = (state: string) =>
  state.includes("clean") ? "#34D399" : state.includes("degraded") || state.includes("undersized") ? "#FBBF24" :
  state.includes("down") || state.includes("stale") || state.includes("incomplete") ? "#F87171" : "#38BDF8";

function bps(n?: number) { return fmtBytes(n || 0) + "/s"; }

export default function Ceph() {
  const { data: st } = useCephStatus();
  const { data: tree } = useCephOsdTree();
  const { data: df } = useCephDf();
  const { data: osdDf } = useCephOsdDf();

  // per-OSD utilization keyed by osd id
  const util: Record<number, any> = {};
  (osdDf?.nodes || []).forEach((o: any) => (util[o.id] = o));
  const fullest = (osdDf?.nodes || []).filter((o: any) => o.status === "up").sort((a: any, b: any) => (b.utilization || 0) - (a.utilization || 0))[0];
  const utilColor = (p: number) => (p >= 85 ? "#F87171" : p >= 70 ? "#FBBF24" : "#34D399");

  const health = st?.health?.status;
  const checks: any[] = st?.health?.checks ? Object.entries(st.health.checks).map(([k, v]: any) => ({ k, ...v })) : [];
  const osd = st?.osdmap || {};
  const pgs: any[] = st?.pgmap?.pgs_by_state || [];
  const totalPg = st?.pgmap?.num_pgs || pgs.reduce((a, p) => a + p.count, 0);
  const io = st?.pgmap;

  // OSD tree → host → osd hierarchy
  const nodes: any[] = tree?.nodes || [];
  const byId: Record<number, any> = {}; nodes.forEach((n) => (byId[n.id] = n));
  const roots = nodes.filter((n) => n.type === "root");

  return (
    <div>
      <PageHeader icon={Aperture} title="Ceph" subtitle="Live cluster health, CRUSH map, placement groups, and pool usage" />

      {/* status row */}
      <div className="grid sm:grid-cols-2 lg:grid-cols-4 gap-3 mb-4">
        <StatCard label="Health" value={<Badge kind={healthKind(health)} dot>{health || "…"}</Badge>} sub={st?.quorum_names ? `mon quorum ${st.quorum_names.length}/${st.monmap?.num_mons ?? "?"}` : ""} />
        <StatCard label="OSDs up / in" value={<>{num(osd.num_up_osds)}<span className="text-muted-foreground text-sm"> / {num(osd.num_in_osds)}</span></>} sub={`${num(osd.num_osds)} total`} />
        {fullest ? (
          <StatCard label="Fullest OSD" value={<span style={{ color: utilColor(fullest.utilization) }}>{fullest.utilization.toFixed(1)}%</span>} sub={`${fullest.name} · avg ${(osdDf?.summary?.average_utilization ?? 0).toFixed(1)}%`} />
        ) : (
          <StatCard label="Placement groups" value={num(totalPg)} sub={`${pgs.length} state(s)`} />
        )}
        <StatCard label="Client I/O" value={<span className="text-lg">{bps(io?.read_bytes_sec)} r</span>} sub={`${bps(io?.write_bytes_sec)} w · ${num(io?.read_op_per_sec)}/${num(io?.write_op_per_sec)} ops`} />
      </div>

      {/* health checks */}
      {checks.length > 0 && (
        <GlassSection title={<>Health checks <Badge kind="warning">{checks.length}</Badge></>}>
          <div className="divide-y divide-white/5">
            {checks.map((c) => (
              <div key={c.k} className="flex items-start gap-3 px-3 py-2.5">
                <Badge kind={c.severity === "HEALTH_ERR" ? "danger" : "warning"}>{c.k}</Badge>
                <span className="text-sm text-muted-foreground">{c.summary?.message || ""}</span>
              </div>
            ))}
          </div>
        </GlassSection>
      )}

      {/* PG states */}
      <GlassSection title="Placement-group states">
        <div className="p-3">
          <div className="flex h-2.5 rounded-full overflow-hidden bg-white/5 mb-3">
            {pgs.map((p) => <div key={p.state_name} style={{ width: `${(p.count / totalPg) * 100}%`, background: PG_COLOR(p.state_name) }} title={`${p.state_name} · ${p.count}`} />)}
          </div>
          <div className="flex flex-wrap gap-2">
            {pgs.map((p) => (
              <span key={p.state_name} className="mono text-xs px-2.5 py-1 rounded-full border border-white/10 flex items-center gap-2">
                <span className="w-2 h-2 rounded-full" style={{ background: PG_COLOR(p.state_name) }} />{p.state_name} <b className="text-foreground">{p.count}</b>
              </span>
            ))}
          </div>
        </div>
      </GlassSection>

      {/* CRUSH / OSD tree */}
      <GlassSection title={<>CRUSH map · OSD tree <Badge kind="neutral">{nodes.filter((n) => n.type === "osd").length} OSDs</Badge></>}>
        <div className="p-3 font-mono text-sm">
          {roots.map((r) => (
            <div key={r.id}>
              <div className="flex items-center gap-2 py-1"><ChevronRight size={14} className="text-muted-foreground" /><span className="text-sky-300">{r.type}</span> {r.name}</div>
              {(r.children || []).map((hid: number) => {
                const host = byId[hid]; if (!host) return null;
                return (
                  <div key={hid} className="ml-5">
                    <div className="flex items-center gap-2 py-1"><ChevronRight size={13} className="text-muted-foreground" /><span className="text-violet-300">host</span> {host.name}</div>
                    <div className="ml-6">
                      {(host.children || []).map((oid: number) => {
                        const o = byId[oid]; if (!o) return null;
                        const up = o.status === "up";
                        const u = util[oid];
                        const up2 = up;
                        return (
                          <div key={oid} className="flex items-center gap-3 py-0.5">
                            <span className="text-muted-foreground w-16">{o.name}</span>
                            <Badge kind={up2 ? "success" : "danger"} dot>{o.status}</Badge>
                            {u && up2 ? (
                              <>
                                <div className="w-28 h-1.5 rounded-full bg-white/5 overflow-hidden">
                                  <div className="h-full rounded-full" style={{ width: `${Math.min(100, u.utilization)}%`, background: utilColor(u.utilization) }} />
                                </div>
                                <span className="text-xs tabular-nums" style={{ color: utilColor(u.utilization) }}>{u.utilization.toFixed(1)}%</span>
                                <span className="text-muted-foreground text-xs">{u.pgs} PGs · {u.device_class}</span>
                              </>
                            ) : (
                              <span className="text-muted-foreground text-xs">weight {(o.crush_weight ?? 0).toFixed?.(1) ?? o.crush_weight} · reweight {o.reweight}</span>
                            )}
                          </div>
                        );
                      })}
                    </div>
                  </div>
                );
              })}
            </div>
          ))}
          {!nodes.length && <div className="text-muted-foreground">No OSD tree.</div>}
        </div>
      </GlassSection>

      {/* pool df */}
      <GlassSection title={<>Pool usage · ceph df <Badge kind="neutral">{df?.pools?.length || 0}</Badge></>}>
        <Table
          rows={df?.pools}
          rowKey={(p: any) => String(p.id)}
          cols={[
            { h: "Pool", f: (p: any) => p.name, mono: true },
            { h: "Objects", f: (p: any) => num(p.stats?.objects) },
            { h: "Stored", f: (p: any) => fmtBytes(p.stats?.stored) },
            { h: "% used", f: (p: any) => ((p.stats?.percent_used || 0) * 100).toFixed(2) + "%" },
            { h: "Max avail", f: (p: any) => fmtBytes(p.stats?.max_avail) },
          ]}
        />
      </GlassSection>
    </div>
  );
}
