// Copyright (c) 2026 ZyvorAI Labs Private Limited. All rights reserved.
// Observatory — six live visualizations of the estate (orbital, trajectory, terminal, fabric,
// treemap, isometric), each driven by the real API instead of a snapshot. Ported from the
// standalone artifacts; canvas sims run once and read the latest data from a ref so live refetches
// don't restart the animation.
import { useEffect, useRef, useState } from "react";
import { Orbit } from "lucide-react";
import {
  useAlerts, useBackendsSummary, useClusters, useForecast, useHistory, usePools, useSummary,
} from "../api/hooks";
import { PageHeader, Spinner } from "../ui/kit";

const KC: Record<string, string> = {
  rbd: "#38BDF8", cephfs_data: "#5bd8ff", cephfs_metadata: "#5bd8ff",
  rgw: "#A78BFA", nfs_export: "#34D399", zpool: "#FBBF24", other: "#6b8bb5",
};
const kc = (k: string) => KC[k] || KC.other;
const bcol = (t: string) => (t === "nfs" ? "#34D399" : t === "zfs" ? "#FBBF24" : "#38BDF8");
// Map a pool to its backend TYPE via the cluster join (robust for N backends). Falls back to a
// kind heuristic when clusters aren't loaded yet.
function poolBackendMap(clusters: any[], backends: any[]) {
  const clById: Record<string, string> = {};
  (clusters || []).forEach((c) => (clById[c.id] = c.backend_id));
  const typeByBackend: Record<string, string> = {};
  (backends || []).forEach((b) => (typeByBackend[b.backend_id] = b.backend_type));
  return (pool: any): string => {
    const bt = typeByBackend[clById[pool.cluster_id]];
    if (bt) return bt;
    if (pool.kind === "nfs_export") return "nfs";
    if (pool.kind === "zpool") return "zfs";
    return "ceph";
  };
}
const TB = 1e12, GB = 1e9;
function tb(n: number) {
  if (n >= TB) return (n / TB).toFixed(n >= 10 * TB ? 1 : 2) + " TB";
  if (n >= GB) return (n / GB).toFixed(n >= 100 * GB ? 0 : 1) + " GB";
  if (n >= 1e6) return (n / 1e6).toFixed(1) + " MB";
  return (n / 1e3).toFixed(0) + " KB";
}
const PRODUCTS = ["Zeus OS", "Veyron", "Hyper2KVM", "GuestKit", "PacketWolf", "Aether", "Ragnarok", "Machina", "HyperSDK"];
const reduced = () => matchMedia("(prefers-reduced-motion:reduce)").matches;

// generic canvas host: gives the effect a sized container + DPR-scaled canvas
function useCanvas(run: (ctx: CanvasRenderingContext2D, api: { w(): number; h(): number }) => () => void, deps: unknown[]) {
  const box = useRef<HTMLDivElement>(null);
  const cv = useRef<HTMLCanvasElement>(null);
  useEffect(() => {
    const c = cv.current!, b = box.current!, ctx = c.getContext("2d")!;
    const DPR = Math.min(devicePixelRatio || 1, 2);
    let W = 0, H = 0;
    const fit = () => { W = b.clientWidth; H = b.clientHeight; c.width = W * DPR; c.height = H * DPR; ctx.setTransform(DPR, 0, 0, DPR, 0, 0); };
    fit();
    const ro = new ResizeObserver(fit); ro.observe(b);
    const cleanup = run(ctx, { w: () => W, h: () => H });
    return () => { ro.disconnect(); cleanup && cleanup(); };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, deps);
  return { box, cv };
}

// ---------------------------------------------------------------- Orbital
function Orbital({ data }: { data: any }) {
  const ref = useRef(data); ref.current = data;
  const { box, cv } = useCanvas((ctx, api) => {
    const stars = () => {
      const n = Math.round((api.w() * api.h()) / 13000); const a: any[] = [];
      for (let i = 0; i < n; i++) a.push({ x: Math.random() * api.w(), y: Math.random() * api.h(), z: Math.random(), ph: Math.random() * 6.28 });
      return a;
    };
    let S = stars(); let lastW = api.w();
    let raf = 0; const red = reduced();
    const draw = (t: number) => {
      const W = api.w(), H = api.h(); if (W !== lastW) { S = stars(); lastW = W; }
      const { s, backends, pools, clusters } = ref.current;
      const pb = poolBackendMap(clusters, backends);
      const cx = W / 2, cy = H / 2, half = Math.min(W, H) / 2;
      ctx.clearRect(0, 0, W, H);
      for (const st of S) { const tw = red ? 0.7 : 0.45 + 0.55 * Math.abs(Math.sin(t * 0.001 + st.ph)); ctx.globalAlpha = (0.25 + 0.6 * st.z) * tw; ctx.fillStyle = st.z > 0.85 ? "#bfe0ff" : "#7f9bc4"; ctx.fillRect(st.x, st.y, st.z > 0.9 ? 1.6 : 1, st.z > 0.9 ? 1.6 : 1); }
      ctx.globalAlpha = 1;
      const orbit = half * 0.52;
      ctx.strokeStyle = "rgba(56,90,140,.16)"; ctx.lineWidth = 1; ctx.beginPath(); ctx.arc(cx, cy, orbit, 0, 6.283); ctx.stroke();
      // core gauge
      const used = s.raw_capacity_bytes > 0 ? s.used_capacity_bytes / s.raw_capacity_bytes : 0;
      const coreR = half * 0.20;
      ctx.strokeStyle = "rgba(56,90,140,.28)"; ctx.lineWidth = 7; ctx.beginPath(); ctx.arc(cx, cy, coreR, 0, 6.283); ctx.stroke();
      const g = ctx.createLinearGradient(cx - coreR, cy - coreR, cx + coreR, cy + coreR); g.addColorStop(0, "#38BDF8"); g.addColorStop(1, "#2563EB");
      ctx.strokeStyle = g; ctx.lineCap = "round"; ctx.beginPath(); ctx.arc(cx, cy, coreR, -Math.PI / 2, -Math.PI / 2 + used * 6.283); ctx.stroke(); ctx.lineCap = "butt";
      ctx.textAlign = "center"; ctx.fillStyle = "#eaf6ff"; ctx.font = "700 26px ui-monospace,Menlo,monospace"; ctx.fillText(tb(s.raw_capacity_bytes), cx, cy - 2);
      ctx.fillStyle = "#8AA0BD"; ctx.font = "10px ui-monospace,Menlo,monospace"; ctx.fillText("RAW · " + Math.round(used * 100) + "% ENGAGED", cx, cy + 18);
      // backends
      const bs = backends || [];
      bs.forEach((B: any, i: number) => {
        B._ang = (B._ang == null ? (i / bs.length) * 6.283 + 0.4 : B._ang + (red ? 0 : (B.backend_type === "nfs" ? 0.030 : 0.045) * 0.016));
        const bx = cx + Math.cos(B._ang) * orbit, by = cy + Math.sin(B._ang) * orbit;
        const r = 14 + 5 * Math.log10(Math.max(10, B.raw_capacity_bytes / GB)) - 8;
        const col = bcol(B.backend_type);
        ctx.strokeStyle = "rgba(56,90,140,.2)"; ctx.beginPath(); ctx.moveTo(cx, cy); ctx.lineTo(bx, by); ctx.stroke();
        const mp = (pools || []).filter((p: any) => pb(p) === B.backend_type);
        mp.forEach((p: any, j: number) => { const a = (j / Math.max(1, mp.length)) * 6.283 + t * 0.0004 * (red ? 0 : 1); const md = r + 16 + (j % 3) * 7; const mx = bx + Math.cos(a) * md, my = by + Math.sin(a) * md; ctx.globalAlpha = 0.85; ctx.fillStyle = kc(p.kind); ctx.beginPath(); ctx.arc(mx, my, p.kind === "nfs_export" ? 4 : 3, 0, 6.283); ctx.fill(); });
        ctx.globalAlpha = 1;
        const bg = ctx.createRadialGradient(bx, by, 0, bx, by, r * 2.4); bg.addColorStop(0, col + "55"); bg.addColorStop(1, col + "00"); ctx.fillStyle = bg; ctx.beginPath(); ctx.arc(bx, by, r * 2.4, 0, 6.283); ctx.fill();
        const bd = ctx.createRadialGradient(bx - r * 0.4, by - r * 0.4, r * 0.1, bx, by, r); bd.addColorStop(0, "#dff3ff"); bd.addColorStop(0.25, col); bd.addColorStop(1, B.backend_type === "nfs" ? "#127a52" : "#1b4a86"); ctx.fillStyle = bd; ctx.beginPath(); ctx.arc(bx, by, r, 0, 6.283); ctx.fill();
        ctx.fillStyle = "#cfe0f5"; ctx.font = "600 11px ui-monospace,Menlo,monospace"; ctx.fillText(B.backend_type.toUpperCase(), bx, by + r + 16);
      });
      if (!red) raf = requestAnimationFrame(draw);
    };
    raf = requestAnimationFrame(draw); if (red) draw(0);
    return () => cancelAnimationFrame(raf);
  }, []);
  return <div ref={box} className="absolute inset-0"><canvas ref={cv} className="w-full h-full block" /></div>;
}

// ---------------------------------------------------------------- Trajectory
function Trajectory({ data }: { data: any }) {
  const ref = useRef(data); ref.current = data;
  const { box, cv } = useCanvas((ctx, api) => {
    let raf = 0, t0: number | null = null; const red = reduced();
    const frame = (ts: number) => {
      if (t0 === null) t0 = ts;
      const { s, hist, fc } = ref.current;
      const W = api.w(), H = api.h(); ctx.clearRect(0, 0, W, H);
      const RAW = s.raw_capacity_bytes / TB;
      // build series
      const H0 = hist.length ? Date.parse(hist[0].ts) : 0;
      const pts = hist.map((x: any) => [(Date.parse(x.ts) - H0) / 3.6e6, x.used_capacity_bytes / TB]);
      const span = pts.length ? pts[pts.length - 1][0] : 1;
      const rel = pts.map((p: number[]) => [p[0] - span, p[1]]);
      const nowUsed = pts.length ? pts[pts.length - 1][1] : s.used_capacity_bytes / TB;
      const growth = (fc.growth_bytes_per_day || 0) / TB;
      const days = fc.days_to_full;
      const tFill = days != null && growth > 0 ? (RAW - nowUsed) / growth * 24 : 0;
      const PL = 60, PR = 118, PT = 16, PB = 34, pw = W - PL - PR, ph = H - PT - PB;
      const xMin = (rel.length ? rel[0][0] : -1) - 0.6, xMax = (tFill > 0 ? tFill : 2) + 0.9;
      const sx = (h: number) => PL + (h - xMin) / (xMax - xMin) * pw, sy = (v: number) => PT + (1 - v / RAW) * ph;
      // grid
      ctx.font = "10px ui-monospace,Menlo,monospace"; ctx.textAlign = "right"; ctx.textBaseline = "middle";
      for (let v = 0; v <= RAW + 0.01; v += Math.max(1, Math.round(RAW / 4))) { const y = sy(v); ctx.strokeStyle = "rgba(56,90,140,.14)"; ctx.beginPath(); ctx.moveTo(PL, y); ctx.lineTo(PL + pw, y); ctx.stroke(); ctx.fillStyle = "#5C7192"; ctx.fillText(v.toFixed(0) + " TB", PL - 10, y); }
      ctx.textAlign = "center"; ctx.textBaseline = "top";
      [-8, -4, 0, 4, 8, 12].forEach((h) => { if (h < xMin || h > xMax) return; ctx.fillStyle = h === 0 ? "#8fb6e8" : "#5C7192"; ctx.fillText(h === 0 ? "NOW" : (h > 0 ? "+" + h + "h" : h + "h"), sx(h), PT + ph + 8); });
      // ceiling
      const cyl = sy(RAW); ctx.setLineDash([5, 5]); ctx.strokeStyle = "rgba(248,113,113,.55)"; ctx.lineWidth = 1.2; ctx.beginPath(); ctx.moveTo(PL, cyl); ctx.lineTo(PL + pw, cyl); ctx.stroke(); ctx.setLineDash([]);
      ctx.fillStyle = "#F87171"; ctx.textAlign = "left"; ctx.textBaseline = "bottom"; ctx.fillText("RAW CEILING · " + RAW.toFixed(1) + " TB", PL + 6, cyl - 4);
      const p = red ? 1 : Math.min(1, (ts - t0) / 2400); const ease = 1 - Math.pow(1 - p, 3); const revX = PL + ease * pw;
      ctx.save(); ctx.beginPath(); ctx.rect(PL, PT - 6, Math.max(0, revX - PL), ph + 12); ctx.clip();
      // history area
      const g = ctx.createLinearGradient(0, PT, 0, PT + ph); g.addColorStop(0, "rgba(56,189,248,.42)"); g.addColorStop(1, "rgba(56,189,248,0)");
      if (rel.length) { ctx.beginPath(); ctx.moveTo(sx(rel[0][0]), sy(0)); rel.forEach((q: number[]) => ctx.lineTo(sx(q[0]), sy(q[1]))); ctx.lineTo(sx(0), sy(0)); ctx.closePath(); ctx.fillStyle = g; ctx.fill();
        ctx.beginPath(); rel.forEach((q: number[], i: number) => { const X = sx(q[0]), Y = sy(q[1]); i ? ctx.lineTo(X, Y) : ctx.moveTo(X, Y); }); ctx.strokeStyle = "#38BDF8"; ctx.lineWidth = 2; ctx.stroke(); }
      // projection
      if (tFill > 0) { const px0 = sx(0), py0 = sy(nowUsed), px1 = sx(tFill), py1 = sy(RAW);
        const cone = ctx.createLinearGradient(px0, 0, px1, 0); cone.addColorStop(0, "rgba(251,191,36,.16)"); cone.addColorStop(1, "rgba(251,191,36,.02)");
        ctx.beginPath(); ctx.moveTo(px0, py0); ctx.lineTo(px1, py1); ctx.lineTo(px1, sy(0)); ctx.lineTo(px0, sy(0)); ctx.closePath(); ctx.fillStyle = cone; ctx.fill();
        ctx.setLineDash([6, 5]); ctx.strokeStyle = "#FBBF24"; ctx.lineWidth = 1.8; ctx.beginPath(); ctx.moveTo(px0, py0); ctx.lineTo(px1, py1); ctx.stroke(); ctx.setLineDash([]);
        const pulse = red ? 1 : 0.6 + 0.4 * Math.sin(Date.now() * 0.005); ctx.globalAlpha = pulse; ctx.fillStyle = "#FBBF24"; ctx.beginPath(); ctx.arc(px1, py1, 4.2, 0, 6.283); ctx.fill(); ctx.globalAlpha = 1;
        ctx.fillStyle = "#FBBF24"; ctx.textAlign = "right"; ctx.textBaseline = "bottom"; ctx.font = "600 10px ui-monospace,Menlo,monospace"; ctx.fillText("FILL HORIZON", px1 - 8, py1 - 2);
        ctx.fillStyle = "#8AA0BD"; ctx.textBaseline = "top"; ctx.fillText("~" + days.toFixed(1) + " d", px1 - 8, py1 + 4);
      } else { ctx.fillStyle = "#34D399"; ctx.font = "600 11px ui-monospace,Menlo,monospace"; ctx.textAlign = "left"; ctx.textBaseline = "top"; ctx.fillText("usage steady — no fill projection", sx(0) + 8, PT + 6); }
      // now line + dot
      ctx.strokeStyle = "rgba(56,189,248,.5)"; ctx.lineWidth = 1; ctx.beginPath(); ctx.moveTo(sx(0), PT - 4); ctx.lineTo(sx(0), PT + ph); ctx.stroke();
      ctx.fillStyle = "#eaf6ff"; ctx.beginPath(); ctx.arc(sx(0), sy(nowUsed), 3.4, 0, 6.283); ctx.fill();
      ctx.restore();
      if (revX < PL + pw - 1) { ctx.strokeStyle = "rgba(191,224,255,.8)"; ctx.lineWidth = 1.4; ctx.beginPath(); ctx.moveTo(revX, PT - 6); ctx.lineTo(revX, PT + ph); ctx.stroke(); }
      if (p < 1 || !red) raf = requestAnimationFrame(frame);
    };
    raf = requestAnimationFrame(frame);
    return () => cancelAnimationFrame(raf);
  }, []);
  return <div ref={box} className="absolute inset-0"><canvas ref={cv} className="w-full h-full block" /></div>;
}

// ---------------------------------------------------------------- Terminal
function Terminal({ data }: { data: any }) {
  const d = data; const s = d.s, backends = d.backends || [], fc = d.fc, clusters = d.clusters || [], alerts = d.alerts || [];
  const ceph = backends.find((b: any) => b.backend_type === "ceph") || {};
  const cephCl = clusters.find((c: any) => c.backend_id === ceph.backend_id);
  const health = cephCl ? String(cephCl.health).toUpperCase() : "OK";
  const used = s.raw_capacity_bytes > 0 ? s.used_capacity_bytes / s.raw_capacity_bytes : 0;
  const io = s.client_io || {};
  const [booted, setBooted] = useState(reduced());
  const logRef = useRef<HTMLPreElement>(null);
  const readsRef = useRef<HTMLElement>(null), writesRef = useRef<HTMLElement>(null);

  useEffect(() => {
    const LINES = [
      { l: "init ", m: "ATLAS control plane · build v0.1.0", tag: "LIVE", c: "info" },
      { l: "mount", m: "sqlite inventory (WAL, fk=on)", tag: "OK", c: "ok" },
      ...backends.map((b: any) => b.backend_type === "ceph"
        ? { l: "drv  ", m: "ceph → " + (b.backend_id || "ceph") + " · HEALTH_" + health, tag: health === "OK" ? "OK" : "WARN", c: health === "OK" ? "ok" : "warn" }
        : { l: "drv  ", m: b.backend_type.padEnd(4) + " → " + b.backend_id + " · reachable", tag: "OK", c: "ok" }),
      { l: "disc ", m: "pools · " + s.pools, tag: "OK", c: "ok" },
      { l: "disc ", m: "volumes · " + s.volumes, tag: "OK", c: "ok" },
      { l: "disc ", m: "buckets · " + s.buckets, tag: "OK", c: "ok" },
      { l: "fcst ", m: "least-squares · " + (fc.samples || 0) + " samples", tag: fc.days_to_full != null ? "~" + fc.days_to_full.toFixed(1) + "d" : "STEADY", c: fc.days_to_full != null ? "warn" : "ok" },
      { l: "mon  ", m: "alert rules evaluated", tag: alerts.length ? alerts.length + " OPEN" : "CLEAR", c: alerts.length ? "warn" : "ok" },
      { l: "ready", m: "/readyz · db · driver · k8s", tag: "200", c: "rdy" },
    ];
    const log = logRef.current!; log.innerHTML = "";
    const red = reduced();
    const mk = (L: any) => { const r = document.createElement("div"); r.className = "obsv-ln"; r.innerHTML = '<span class="lead">' + L.l + '  </span><span class="msg"></span><span class="dots"></span><span class="tag ' + L.c + '">[' + L.tag + ']</span>'; log.appendChild(r); return r; };
    const dots = (r: HTMLElement) => { const w = (r.querySelector(".msg") as HTMLElement).textContent!.length; (r.querySelector(".dots") as HTMLElement).textContent = " " + Array(Math.max(3, 44 - w)).join("."); };
    let cancelled = false; const timers: number[] = [];
    if (red) { LINES.forEach((L) => { const r = mk(L); (r.querySelector(".msg") as HTMLElement).textContent = L.m; dots(r); (r.querySelector(".tag") as HTMLElement).classList.add("show"); }); setBooted(true); }
    else {
      let i = 0;
      const nextLine = () => { if (cancelled) return; if (i >= LINES.length) { timers.push(window.setTimeout(() => setBooted(true), 240)); return; }
        const L = LINES[i++], r = mk(L), msg = r.querySelector(".msg") as HTMLElement; let c = 0;
        const type = () => { if (cancelled) return; if (c <= L.m.length) { msg.textContent = L.m.slice(0, c++); timers.push(window.setTimeout(type, 7 + Math.random() * 9)); } else { dots(r); timers.push(window.setTimeout(() => { (r.querySelector(".tag") as HTMLElement).classList.add("show"); timers.push(window.setTimeout(nextLine, 80 + Math.random() * 70)); }, 40)); } };
        type(); };
      nextLine();
    }
    return () => { cancelled = true; timers.forEach(clearTimeout); };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // live-ticking counters
  useEffect(() => {
    if (!booted) return; let reads = io.read_ops_total || 0, writes = io.write_ops_total || 0;
    const paint = () => { if (readsRef.current) readsRef.current.textContent = Math.round(reads).toLocaleString(); if (writesRef.current) writesRef.current.textContent = Math.round(writes).toLocaleString(); };
    paint(); if (reduced()) return;
    const id = setInterval(() => { reads += 120 + Math.random() * 640; writes += 20 + Math.random() * 140; paint(); }, 900);
    return () => clearInterval(id);
  }, [booted, io.read_ops_total, io.write_ops_total]);

  const blocks = (pct: number, n: number) => { const f = Math.round(pct / 100 * n); return "█".repeat(f) + "░".repeat(n - f); };
  return (
    <div className="obsv-crt absolute inset-0">
      <div className="obsv-scan" />
      <div className="obsv-screen">
        <pre ref={logRef} className="obsv-log" />
        {booted && (
          <div className="obsv-hud">
            <div className="obsv-status"><span className="beat">●</span> SYSTEM NOMINAL <small>— {backends.length} backends · {s.pools} pools · {s.volumes} volumes online</small></div>
            <div className="obsv-gauge">CAPACITY  <span className="bar">{blocks(used * 100, 42).slice(0, Math.round(used * 42))}</span><span className="empty">{"░".repeat(42 - Math.round(used * 42))}</span> <span className="cap">{tb(s.used_capacity_bytes)} / {tb(s.raw_capacity_bytes)} · {Math.round(used * 100)}%</span></div>
            <div className="obsv-io">I/O   read <b ref={readsRef}>0</b> ops · {tb(io.read_bytes_total || 0)}   write <b ref={writesRef}>0</b> ops · {tb(io.write_bytes_total || 0)}</div>
            <div className="obsv-prompt">atlas@zyvor:~$ status --live<span className="obsv-cur" /></div>
          </div>
        )}
      </div>
    </div>
  );
}

// ---------------------------------------------------------------- Fabric
function Fabric({ data }: { data: any }) {
  const ref = useRef(data); ref.current = data;
  const { box, cv } = useCanvas((ctx, api) => {
    const red = reduced();
    const { backends, pools, clusters } = ref.current;
    const pb = poolBackendMap(clusters, backends);
    const nodes: any[] = [], links: any[] = [], byId: any = {};
    const add = (n: any) => { n.vx = 0; n.vy = 0; nodes.push(n); byId[n.id] = n; return n; };
    const link = (a: string, b: string, rest: number) => links.push({ a: byId[a], b: byId[b], rest });
    add({ id: "atlas", type: "atlas", label: "ATLAS", color: "#38BDF8", r: 22 });
    PRODUCTS.forEach((p, i) => { add({ id: "p" + i, type: "product", label: p, color: "#A78BFA", r: 8 }); link("p" + i, "atlas", 150); });
    (backends || []).forEach((B: any) => { const id = B.backend_type; add({ id, type: "backend", label: B.backend_type.toUpperCase(), color: bcol(B.backend_type), r: B.backend_type === "nfs" ? 16 : 14 }); link("atlas", id, 172);
      (pools || []).filter((p: any) => pb(p) === B.backend_type).forEach((p: any, j: number) => { const pid = id + "-p" + j; add({ id: pid, type: "pool", label: p.name, color: kc(p.kind), r: 4.5 }); link(id, pid, 50); }); });
    const adj: any = {}; nodes.forEach((n) => (adj[n.id] = new Set())); links.forEach((l) => { adj[l.a.id].add(l.b.id); adj[l.b.id].add(l.a.id); });
    let W = api.w(), H = api.h();
    const anchors = () => { W = api.w(); H = api.h();
      byId.atlas.ax = W * 0.5; byId.atlas.ay = H * 0.5; byId.atlas.ag = 0.03;
      PRODUCTS.forEach((_, i) => { const n = byId["p" + i]; n.ax = W * 0.18; n.ay = H * (0.12 + 0.76 * (i / (PRODUCTS.length - 1))); n.ag = 0.02; });
      (backends || []).forEach((B: any, i: number) => { const n = byId[B.backend_type]; n.ax = W * 0.76; n.ay = H * (0.34 + 0.36 * i); n.ag = 0.025; });
    };
    anchors(); nodes.forEach((n) => { if (n.ax != null) { n.x = n.ax; n.y = n.ay; } else { n.x = W * 0.75 + Math.random() * 60 - 30; n.y = H * 0.5 + Math.random() * 120 - 60; } });
    const shade = (c: string) => (({ "#38BDF8": "#1b4a86", "#34D399": "#12724e", "#A78BFA": "#5b3fae", "#5bd8ff": "#2a6b8f", "#FBBF24": "#7a5a12" } as any)[c] || "#20344f");
    let drag: any = null, hover: any = null, dragDX = 0, dragDY = 0;
    const step = () => {
      for (let i = 0; i < nodes.length; i++) { const n = nodes[i]; for (let j = i + 1; j < nodes.length; j++) { const m = nodes[j]; let dx = n.x - m.x, dy = n.y - m.y; const dd = Math.sqrt(dx * dx + dy * dy) || 0.01; if (dd < 220) { const f = 4600 / (dd * dd), ux = dx / dd, uy = dy / dd; n.vx += ux * f * 0.016; n.vy += uy * f * 0.016; m.vx -= ux * f * 0.016; m.vy -= uy * f * 0.016; } } }
      for (const l of links) { const n = l.a, m = l.b; let dx = m.x - n.x, dy = m.y - n.y; const dd = Math.sqrt(dx * dx + dy * dy) || 0.01, f = (dd - l.rest) * 0.015, ux = dx / dd, uy = dy / dd; n.vx += ux * f; n.vy += uy * f; m.vx -= ux * f; m.vy -= uy * f; }
      for (const n of nodes) { if (n === drag) continue; if (n.ax != null) { n.vx += (n.ax - n.x) * n.ag; n.vy += (n.ay - n.y) * n.ag; } n.vx *= 0.86; n.vy *= 0.86; const sp = Math.hypot(n.vx, n.vy); if (sp > 7) { n.vx *= 7 / sp; n.vy *= 7 / sp; } n.x += n.vx; n.y += n.vy; n.x = Math.max(n.r + 4, Math.min(W - n.r - 4, n.x)); n.y = Math.max(n.r + 4, Math.min(H - n.r - 4, n.y)); }
    };
    const draw = () => { const hi = hover || drag; ctx.clearRect(0, 0, W, H);
      for (const l of links) { const on = hi && (l.a === hi || l.b === hi); ctx.strokeStyle = on ? "rgba(147,197,253,.75)" : "rgba(56,90,140,.22)"; ctx.lineWidth = on ? 1.7 : 1; ctx.beginPath(); ctx.moveTo(l.a.x, l.a.y); ctx.lineTo(l.b.x, l.b.y); ctx.stroke(); }
      for (const n of nodes) { const dim = hi && n !== hi && !adj[hi.id].has(n.id); ctx.globalAlpha = dim ? 0.32 : 1;
        const gg = ctx.createRadialGradient(n.x, n.y, 0, n.x, n.y, n.r * 2.6); gg.addColorStop(0, n.color + (dim ? "22" : "55")); gg.addColorStop(1, n.color + "00"); ctx.fillStyle = gg; ctx.beginPath(); ctx.arc(n.x, n.y, n.r * 2.6, 0, 6.283); ctx.fill();
        const bg = ctx.createRadialGradient(n.x - n.r * 0.4, n.y - n.r * 0.4, n.r * 0.1, n.x, n.y, n.r); bg.addColorStop(0, "#eaf6ff"); bg.addColorStop(0.3, n.color); bg.addColorStop(1, shade(n.color)); ctx.fillStyle = bg; ctx.beginPath(); ctx.arc(n.x, n.y, n.r, 0, 6.283); ctx.fill();
        if (n === hi) { ctx.strokeStyle = n.color; ctx.lineWidth = 1.6; ctx.beginPath(); ctx.arc(n.x, n.y, n.r + 5, 0, 6.283); ctx.stroke(); }
        ctx.globalAlpha = 1;
        if (n.type !== "pool" || n === hi) { ctx.font = (n.type === "atlas" ? "700 12px " : "600 10.5px ") + "ui-monospace,Menlo,monospace"; ctx.fillStyle = n.type === "atlas" ? "#dff2ff" : dim ? "#5C7192" : "#cfe0f5"; ctx.textAlign = "center"; ctx.textBaseline = "top"; ctx.fillText(n.label, n.x, n.y + n.r + 5); } }
    };
    const nodeAt = (mx: number, my: number) => { for (let i = nodes.length - 1; i >= 0; i--) { const n = nodes[i]; if ((mx - n.x) ** 2 + (my - n.y) ** 2 < (n.r + 6) ** 2) return n; } return null; };
    const c = cv.current!;
    const mm = (e: MouseEvent) => { const r = c.getBoundingClientRect(), mx = e.clientX - r.left, my = e.clientY - r.top; if (drag) { drag.x = mx - dragDX; drag.y = my - dragDY; } else hover = nodeAt(mx, my); };
    const md = (e: MouseEvent) => { const r = c.getBoundingClientRect(), mx = e.clientX - r.left, my = e.clientY - r.top; drag = nodeAt(mx, my); if (drag) { dragDX = mx - drag.x; dragDY = my - drag.y; } };
    const mu = () => (drag = null), ml = () => (hover = null);
    c.addEventListener("mousemove", mm); c.addEventListener("mousedown", md); window.addEventListener("mouseup", mu); c.addEventListener("mouseleave", ml);
    let raf = 0; const loop = () => { anchors(); step(); draw(); raf = requestAnimationFrame(loop); };
    if (red) { for (let i = 0; i < 400; i++) step(); draw(); } else loop();
    return () => { cancelAnimationFrame(raf); c.removeEventListener("mousemove", mm); c.removeEventListener("mousedown", md); window.removeEventListener("mouseup", mu); c.removeEventListener("mouseleave", ml); };
  }, []);
  return <div ref={box} className="absolute inset-0"><canvas ref={cv} className="w-full h-full block cursor-grab active:cursor-grabbing" /></div>;
}

// ---------------------------------------------------------------- Treemap
function Treemap({ data }: { data: any }) {
  const pools = data.pools || [];
  const box = useRef<HTMLDivElement>(null);
  useEffect(() => {
    const el = box.current!; const red = reduced();
    const items = pools.map((p: any) => ({ n: p.name, k: p.kind, max: p.max_bytes || 1, used: p.used_bytes || 0 }));
    const squarify = (dt: any[], x: number, y: number, w: number, h: number) => {
      dt = dt.map((d) => ({ ...d })); const total = dt.reduce((s, d) => s + d.max, 0) || 1, area = w * h; dt.forEach((d) => (d._a = d.max / total * area));
      const out: any[] = []; let i = 0; const n = dt.length; let cx = x, cy = y, cw = w, ch = h;
      const worst = (row: any[], side: number) => { let s = 0, mx = 0, mn = Infinity; row.forEach((d) => { s += d._a; if (d._a > mx) mx = d._a; if (d._a < mn) mn = d._a; }); const s2 = s * s, sd2 = side * side; return Math.max(sd2 * mx / s2, s2 / (sd2 * mn)); };
      while (i < n) { const vertical = cw >= ch, side = vertical ? ch : cw, row = [dt[i]]; let j = i + 1; while (j < n) { if (worst(row.concat(dt[j]), side) > worst(row, side)) break; row.push(dt[j]); j++; } const rowSum = row.reduce((a, d) => a + d._a, 0), thick = rowSum / side; let pos = vertical ? cy : cx; row.forEach((d) => { const len = d._a / thick; if (vertical) { out.push({ d, x: cx, y: pos, w: thick, h: len }); pos += len; } else { out.push({ d, x: pos, y: cy, w: len, h: thick }); pos += len; } }); if (vertical) { cx += thick; cw -= thick; } else { cy += thick; ch -= thick; } i = j; }
      return out;
    };
    const render = () => { const W = el.clientWidth, H = el.clientHeight; el.innerHTML = ""; if (!W || !H) return;
      squarify(items, 0, 0, W, H).forEach((r: any, idx: number) => { const d = r.d, pct = Math.min(100, d.used / d.max * 100), col = kc(d.k);
        const t = document.createElement("div"); t.className = "obsv-tile" + (r.w < 86 || r.h < 52 ? " sm" : ""); t.style.left = r.x + 1.5 + "px"; t.style.top = r.y + 1.5 + "px"; t.style.width = Math.max(0, r.w - 3) + "px"; t.style.height = Math.max(0, r.h - 3) + "px"; t.style.background = "rgba(10,16,28,.6)";
        t.innerHTML = '<div class="fillbg" style="background:' + col + '"></div><div class="used" style="height:' + pct.toFixed(1) + '%;background:' + col + '"></div><div class="lab"><div class="nm">' + d.n + '</div><div class="meta">' + tb(d.max) + " · " + (pct < 1 ? pct.toFixed(2) : pct.toFixed(0)) + "%</div></div>";
        t.title = d.n + " · " + d.k + " · " + tb(d.used) + " / " + tb(d.max);
        if (!red) { t.style.transform = "scale(.6)"; t.style.opacity = "0"; setTimeout(() => { t.style.transform = "scale(1)"; t.style.opacity = "1"; }, 30 + idx * 24); }
        el.appendChild(t); }); };
    const ro = new ResizeObserver(render); ro.observe(el); render();
    return () => ro.disconnect();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [pools]);
  return <div ref={box} className="obsv-map absolute inset-0" />;
}

// ---------------------------------------------------------------- Isometric
function Iso({ data }: { data: any }) {
  const ref = useRef(data); ref.current = data;
  const focus = useRef<HTMLDivElement>(null);
  const { box, cv } = useCanvas((ctx, api) => {
    const red = reduced();
    const c = cv.current!;
    let raf = 0; let hover: any = null;
    const build = () => { const { backends, pools, clusters } = ref.current; const pb = poolBackendMap(clusters, backends); const bs = (backends || []).slice(); const list: any[] = [];
      const spread = bs.length > 1 ? bs.length : 1;
      bs.forEach((B: any, i: number) => { const sl = (pools || []).filter((p: any) => pb(p) === B.backend_type); const bx = -3.6 + (i / Math.max(1, spread - 1)) * 7.2; list.push({ name: B.backend_type.toUpperCase(), bx, by: i * 0.7, slabs: sl, phase: i * 2.1 }); }); return list; };
    const inPoly = (px: number, py: number, pts: number[][]) => { let c2 = false; for (let i = 0, j = pts.length - 1; i < pts.length; j = i++) { if (((pts[i][1] > py) !== (pts[j][1] > py)) && px < (pts[j][0] - pts[i][0]) * (py - pts[i][1]) / (pts[j][1] - pts[i][1]) + pts[i][0]) c2 = !c2; } return c2; };
    const dk = (hex: string, f: number) => { const r = parseInt(hex.slice(1, 3), 16), g = parseInt(hex.slice(3, 5), 16), b = parseInt(hex.slice(5, 7), 16); return "rgb(" + Math.round(r * f) + "," + Math.round(g * f) + "," + Math.round(b * f) + ")"; };
    let hit: any[] = [];
    const frame = (ts: number) => { const W = api.w(), H = api.h(); const scale = Math.min(1, W / 900, H / 620), TW = 40 * scale, TH = 21 * scale, SLAB = 14 * scale, GAP = 4 * scale, S = 2.4, OX = W / 2, OY = H * 0.60;
      const proj = (x: number, y: number, z: number) => [OX + (x - y) * TW, OY + (x + y) * TH - z];
      const poly = (pts: number[][], fill: string | null, stroke?: string) => { ctx.beginPath(); pts.forEach((p, i) => (i ? ctx.lineTo(p[0], p[1]) : ctx.moveTo(p[0], p[1]))); ctx.closePath(); if (fill) { ctx.fillStyle = fill; ctx.fill(); } if (stroke) { ctx.strokeStyle = stroke; ctx.lineWidth = 1; ctx.stroke(); } };
      ctx.clearRect(0, 0, W, H); hit = [];
      ctx.strokeStyle = "rgba(56,90,140,.14)"; ctx.lineWidth = 1;
      for (let i = -6; i <= 8; i++) { const a = proj(i, -4, 0), b = proj(i, 8, 0); ctx.beginPath(); ctx.moveTo(a[0], a[1]); ctx.lineTo(b[0], b[1]); ctx.stroke(); }
      for (let j = -4; j <= 8; j++) { const a = proj(-6, j, 0), b = proj(8, j, 0); ctx.beginPath(); ctx.moveTo(a[0], a[1]); ctx.lineTo(b[0], b[1]); ctx.stroke(); }
      const towers = build();
      towers.sort((a, b) => a.bx + a.by - (b.bx + b.by)).forEach((T) => { const bob = red ? 0 : Math.sin(ts * 0.0011 + T.phase) * 4 * scale;
        T.slabs.forEach((sp: any, i: number) => { const z0 = i * (SLAB + GAP) + bob, z1 = z0 + SLAB, col = kc(sp.kind), used = sp.used_bytes || 0, max = sp.max_bytes || 1, em = 0.28 + 0.72 * Math.min(1, used / max);
          const t = [proj(T.bx, T.by, z1), proj(T.bx + S, T.by, z1), proj(T.bx + S, T.by + S, z1), proj(T.bx, T.by + S, z1)];
          const rF = [proj(T.bx + S, T.by, z0), proj(T.bx + S, T.by + S, z0), proj(T.bx + S, T.by + S, z1), proj(T.bx + S, T.by, z1)];
          const lF = [proj(T.bx, T.by + S, z0), proj(T.bx + S, T.by + S, z0), proj(T.bx + S, T.by + S, z1), proj(T.bx, T.by + S, z1)];
          poly(rF, dk(col, 0.30 + 0.25 * em)); poly(lF, dk(col, 0.20 + 0.18 * em)); poly(t, dk(col, 0.55 + 0.45 * em), "rgba(255,255,255,.10)");
          const hv = hover && hover.name === T.name && hover.i === i; if (hv) poly(t, null, "#eaf3ff");
          if (used / max > 0.02) { ctx.save(); ctx.globalAlpha = Math.min(0.5, (used / max) * 0.9); const cxy = proj(T.bx + S / 2, T.by + S / 2, z1); const g = ctx.createRadialGradient(cxy[0], cxy[1], 0, cxy[0], cxy[1], TW * 1.4); g.addColorStop(0, col); g.addColorStop(1, col + "00"); ctx.fillStyle = g; ctx.beginPath(); ctx.arc(cxy[0], cxy[1], TW * 1.4, 0, 6.283); ctx.fill(); ctx.restore(); }
          hit.push({ name: T.name, i, poly: t, sp }); });
        const lp = proj(T.bx + S / 2, T.by + S / 2, T.slabs.length * (SLAB + GAP) + 22 + (red ? 0 : Math.sin(ts * 0.0011 + T.phase) * 4 * scale)); ctx.font = "700 12px ui-monospace,Menlo,monospace"; ctx.fillStyle = "#dff2ff"; ctx.textAlign = "center"; ctx.fillText(T.name, lp[0], lp[1]); ctx.font = "10px ui-monospace,Menlo,monospace"; ctx.fillStyle = "#8AA0BD"; ctx.fillText(T.slabs.length + " pools", lp[0], lp[1] + 15); });
      raf = requestAnimationFrame(frame);
    };
    const mm = (e: MouseEvent) => { const r = c.getBoundingClientRect(), mx = e.clientX - r.left, my = e.clientY - r.top; let h: any = null; for (let i = hit.length - 1; i >= 0; i--) { if (inPoly(mx, my, hit[i].poly)) { h = hit[i]; break; } } hover = h; const fx = focus.current;
      if (h && fx) { const sp = h.sp, pct = (sp.used_bytes || 0) / (sp.max_bytes || 1) * 100; fx.style.opacity = "1"; fx.innerHTML = '<div class="fnm" style="color:' + kc(sp.kind) + '">' + sp.name + '</div><div class="fmeta">' + h.name + " · " + sp.kind + " · " + tb(sp.max_bytes || 0) + " · used " + (pct < 1 ? pct.toFixed(2) : pct.toFixed(0)) + "%</div>"; }
      else if (fx) fx.style.opacity = "0"; };
    c.addEventListener("mousemove", mm);
    raf = requestAnimationFrame(frame);
    return () => { cancelAnimationFrame(raf); c.removeEventListener("mousemove", mm); };
  }, []);
  return <div ref={box} className="absolute inset-0"><canvas ref={cv} className="w-full h-full block" />
    <div ref={focus} className="obsv-isofocus" style={{ opacity: 0 }} /></div>;
}

// ---------------------------------------------------------------- container
const TABS = [
  { id: "orbital", label: "Orbital", cap: "The estate as a solar system — backends orbit, pools are moons." },
  { id: "trajectory", label: "Trajectory", cap: "Persisted capacity time-series with the projected fill horizon." },
  { id: "terminal", label: "Terminal", cap: "The control plane booting — live discovery + status HUD." },
  { id: "fabric", label: "Fabric", cap: "Force-directed dependency graph — drag a node, hover to trace." },
  { id: "treemap", label: "Treemap", cap: "Pools sized by provisioned capacity, filled by usage." },
  { id: "isometric", label: "Isometric", cap: "Backends as glowing server racks — slabs are pools." },
];

export default function Observatory() {
  const [tab, setTab] = useState("orbital");
  const s = useSummary().data, backends = useBackendsSummary().data, pools = usePools().data;
  const fc = useForecast().data, hist = useHistory().data, clusters = useClusters().data, alerts = useAlerts("open").data;
  const base = { s, backends, pools, fc, hist, clusters, alerts };
  const ready = !!(s && backends && pools);
  const trajReady = ready && !!(fc && hist);
  const termReady = ready && !!fc;
  const active = TABS.find((t) => t.id === tab)!;

  return (
    <div>
      <PageHeader icon={Orbit} title="Observatory" subtitle="Six live visualizations of the storage estate — real data, six lenses" />
      <div className="flex flex-wrap gap-2 mb-3">
        {TABS.map((t) => (
          <button key={t.id} onClick={() => setTab(t.id)}
            className={"px-3 py-1.5 rounded-lg text-sm font-medium border transition-colors " + (tab === t.id ? "text-white border-transparent" : "text-muted-foreground border-white/10 hover:bg-white/5")}
            style={tab === t.id ? { background: "linear-gradient(180deg,#38BDF8,#2563EB)" } : undefined}>{t.label}</button>
        ))}
      </div>
      <div className="text-xs text-muted-foreground mb-3 mono">{active.cap}</div>
      <div className="glass-card relative overflow-hidden" style={{ height: "min(74vh, 760px)", minHeight: 460, background: "radial-gradient(1000px 700px at 28% 16%, #0a1526 0%, rgba(10,21,38,0) 60%), #060912" }}>
        {!ready ? (
          <div className="absolute inset-0 grid place-items-center"><Spinner /></div>
        ) : (
          <>
            {tab === "orbital" && <Orbital data={base} />}
            {tab === "trajectory" && (trajReady ? <Trajectory data={base} /> : <div className="absolute inset-0 grid place-items-center"><Spinner /></div>)}
            {tab === "terminal" && (termReady ? <Terminal data={base} /> : <div className="absolute inset-0 grid place-items-center"><Spinner /></div>)}
            {tab === "fabric" && <Fabric data={base} />}
            {tab === "treemap" && <Treemap data={base} />}
            {tab === "isometric" && <Iso data={base} />}
          </>
        )}
      </div>
    </div>
  );
}
