// Copyright (c) 2026 ZyvorAI Labs Private Limited. All rights reserved.
// Pool Detail — Soundings archetype C (instruments → cross-section → seabed → ledger).
import { useEffect, useMemo } from "react";
import { Link, useNavigate, useParams } from "react-router-dom";
import { useAlerts, useClusters, useOsds, usePools, useVolumes } from "../api/hooks";
import { depth, depthWidth } from "../lib/depth";
import { fmtBytes, fmtPct } from "../lib/format";
import { sendPrompt } from "../lib/prompts";
import { PageHead } from "../ui/PageHead";

export default function PoolDetail() {
  const { id = "" } = useParams();
  const nav = useNavigate();
  const { data: pools } = usePools();
  const { data: clusters } = useClusters();
  const { data: vols } = useVolumes();
  const { data: osds } = useOsds();
  const { data: alerts } = useAlerts("open");

  const pool = useMemo(() => (pools || []).find((p) => p.id === id), [pools, id]);
  const cluster = useMemo(
    () => (clusters || []).find((c) => c.id === pool?.cluster_id),
    [clusters, pool],
  );
  const poolVols = useMemo(
    () => (vols || []).filter((v) => v.pool_id === id || (!v.pool_id && pool && v.name.includes(pool.name))),
    [vols, id, pool],
  );

  useEffect(() => {
    if (pool) document.title = `Atlas · ${pool.name}`;
  }, [pool]);

  const used = pool?.used_bytes ?? 0;
  const max = pool?.max_bytes ?? 0;
  const free = Math.max(0, max - used);
  const pct = max > 0 ? (used / max) * 100 : 0;
  const d = depth(pct);

  const stateLine = !pool
    ? pools
      ? "Pool not found in inventory — it may have been removed from discovery."
      : "Sounding this pool…"
    : pool.health !== "ok"
      ? `${pool.name} reports ${pool.health.toUpperCase()} at ${fmtPct(pct)}% · ${d.name}.`
      : pct >= 75
        ? `${pool.name} is ${d.name} — ${fmtPct(pct)}% of ${fmtBytes(max)} provisioned.`
        : `${pool.name} · ${pool.kind} · ${fmtPct(pct)}% occupied · health ${pool.health}.`;

  const ledger = useMemo(() => {
    const items: { t: string; cls: string; text: string }[] = [];
    for (const a of (alerts || []).slice(0, 5)) {
      if (
        a.resource_id === id ||
        a.resource_id === pool?.name ||
        a.title.toLowerCase().includes((pool?.name || "").toLowerCase())
      ) {
        items.push({
          t: a.created_at
            ? new Date(a.created_at).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" })
            : "—",
          cls: a.severity === "critical" ? "dp5" : a.severity === "warning" ? "dp4" : "dp3",
          text: a.title,
        });
      }
    }
    if (!items.length && pool) {
      items.push({
        t: "now",
        cls: d.cls,
        text: `Depth ${d.name} · ${fmtBytes(used)} of ${fmtBytes(max)}`,
      });
    }
    return items;
  }, [alerts, id, pool, d.cls, d.name, used, max]);

  const seabed = useMemo(() => {
    if (poolVols.length) {
      return poolVols.slice(0, 48).map((v) => {
        const p = v.size_bytes > 0 && v.used_bytes != null ? (v.used_bytes / v.size_bytes) * 100 : 0;
        return { cls: depth(p).cls, title: `${v.name} · ${v.state}` };
      });
    }
    const rows = osds || [];
    const n = Math.max(rows.length, 24);
    return Array.from({ length: n }, (_, i) => {
      const o = rows[i % Math.max(rows.length, 1)];
      const cls = !o ? "dp2" : !o.up ? "dp5" : o.in_cluster ? "dp1" : "dp3";
      return { cls, title: o ? `osd.${o.osd_num ?? o.id}` : `cell ${i}` };
    });
  }, [poolVols, osds]);

  const waterY = 18 + (100 - Math.max(pct, 1.5)) * 0.9;

  if (!pool && pools) {
    return (
      <div>
        <PageHead
          crumbs={[
            { label: "Storage", to: "/" },
            { label: "Pools", to: "/ceph" },
            { label: "Not found" },
          ]}
          eyebrow="POOL · DETAIL"
          title="Pool not found"
          state={stateLine}
          actions={
            <button type="button" className="at-btn" onClick={() => nav("/")}>
              Command Deck
            </button>
          }
        />
      </div>
    );
  }

  return (
    <div>
      <PageHead
        crumbs={[
          { label: "Storage", to: "/" },
          { label: "Pools", to: "/ceph" },
          { label: pool?.name || id },
        ]}
        eyebrow={
          <>
            POOL · {cluster?.name || pool?.cluster_id || "…"}
            {pool ? ` · ${pool.kind}` : ""}
          </>
        }
        title={pool?.name || "Pool"}
        state={stateLine}
        actions={
          <>
            <button type="button" className="at-btn" onClick={() => nav("/")}>
              Deck
            </button>
            <Link to="/volumes" className="at-btn">
              Volumes
            </Link>
            <button
              type="button"
              className="at-btn primary"
              onClick={() =>
                sendPrompt(
                  `Analyse pool ${pool?.name || id}: depth ${fmtPct(pct)}%, health ${pool?.health}, and recommend capacity actions.`,
                )
              }
            >
              Ask Atlas
            </button>
          </>
        }
      />

      <div className="at-instrs">
        <div className="at-instr">
          <div className="at-caption">Used</div>
          <div className="at-val md">
            {fmtBytes(used).replace(/ .*/, "")}
            <span className="at-unit">{fmtBytes(used).split(" ").slice(-1)[0]}</span>
          </div>
          <div className="at-delta">{fmtPct(pct)}% of provisioned</div>
        </div>
        <div className="at-instr">
          <div className="at-caption">Free</div>
          <div className="at-val md">
            {fmtBytes(free).replace(/ .*/, "")}
            <span className="at-unit">{fmtBytes(free).split(" ").slice(-1)[0]}</span>
          </div>
          <div className="at-delta">{max ? `${fmtPct(100 - pct)}% remaining` : "capacity unknown"}</div>
        </div>
        <div className="at-instr">
          <div className="at-caption">Health</div>
          <div className="at-val md" style={{ textTransform: "uppercase" }}>
            {pool?.health || "—"}
          </div>
          <div className="at-delta">cluster {cluster?.health || "—"}</div>
        </div>
        <div className="at-instr">
          <div className="at-caption">Replica / class</div>
          <div className="at-val md">{pool?.replica_size ?? "—"}</div>
          <div className="at-delta mono" style={{ fontSize: 11 }}>
            {pool?.device_class || pool?.kind || "—"}
          </div>
        </div>
      </div>

      <div className="at-mod">
        <div className="at-modhead">
          <span className="at-modtitle">Cross-section</span>
          <span className="at-modnote">
            {d.name} · {fmtPct(pct)}%
          </span>
        </div>
        <div className="at-cross">
          <svg className="at-cross-svg" viewBox="0 0 800 140" preserveAspectRatio="none" aria-hidden>
            <defs>
              <linearGradient id="poolFill" x1="0" y1="0" x2="0" y2="1">
                <stop offset="0%" stopColor="var(--d2)" stopOpacity=".65" />
                <stop offset="100%" stopColor="var(--d3)" stopOpacity=".3" />
              </linearGradient>
            </defs>
            <path
              d="M0 28 C120 10, 200 40, 320 22 S520 8, 640 30 S760 20, 800 26 L800 140 L0 140 Z"
              fill="rgba(16,28,37,.9)"
              stroke="rgba(120,180,200,.18)"
            />
            <path
              d={`M0 ${waterY + 40} C160 ${waterY + 20}, 320 ${waterY + 50}, 480 ${waterY + 28} S700 ${waterY + 36}, 800 ${waterY + 44} L800 140 L0 140 Z`}
              fill="url(#poolFill)"
            />
            <line
              x1="0"
              y1={waterY + 40}
              x2="800"
              y2={waterY + 44}
              stroke="var(--d1)"
              strokeWidth="1.6"
              opacity=".95"
            />
            <text x="16" y="18" fill="var(--at-ink-3)" style={{ fontFamily: "var(--at-mono)", fontSize: 11 }}>
              waterline · {fmtBytes(used)} / {fmtBytes(max)}
            </text>
          </svg>
          <div className={`at-mini ${d.cls}`} style={{ width: "100%", height: 10, marginTop: 12 }}>
            <i style={{ width: depthWidth(pct) }} />
          </div>
        </div>
      </div>

      <div className="at-2col">
        <div className="at-mod">
          <div className="at-modhead">
            <span className="at-modtitle">Seabed</span>
            <span className="at-modnote">
              {poolVols.length ? `${poolVols.length} volumes` : `${(osds || []).length || "—"} OSD health tiles`}
            </span>
          </div>
          <div className="at-seabed">
            {seabed.map((c, i) => (
              <span key={i} className={`at-pg ${c.cls}`} title={c.title} />
            ))}
          </div>
          {!poolVols.length ? (
            <div className="at-fill-hint" style={{ marginTop: 10 }}>
              No volumes bound to this pool id yet — tiles show OSD health as a proxy.
            </div>
          ) : null}
        </div>
        <div className="at-mod">
          <div className="at-modhead">
            <span className="at-modtitle">Ledger</span>
            <span className="at-modnote">alerts · soundings</span>
          </div>
          <div className="at-ledger">
            {ledger.map((row, i) => (
              <div key={i} className="at-entry">
                <time>{row.t}</time>
                <span className={`mk ${row.cls}`} />
                <span className="txt">{row.text}</span>
              </div>
            ))}
          </div>
        </div>
      </div>

      {poolVols.length > 0 && (
        <div className="at-mod">
          <div className="at-modhead">
            <span className="at-modtitle">Volumes on this pool</span>
            <span className="at-modnote">{poolVols.length}</span>
          </div>
          <div className="at-panel">
            <table className="at-tbl">
              <thead>
                <tr>
                  <th>Name</th>
                  <th>Kind</th>
                  <th>Size</th>
                  <th>State</th>
                </tr>
              </thead>
              <tbody>
                {poolVols.slice(0, 20).map((v) => (
                  <tr key={v.id} onClick={() => nav(`/volumes?focus=${v.id}`)} style={{ cursor: "pointer" }}>
                    <td className="mono">{v.name}</td>
                    <td>{v.kind}</td>
                    <td className="mono">{fmtBytes(v.size_bytes)}</td>
                    <td>{v.state}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        </div>
      )}
    </div>
  );
}
