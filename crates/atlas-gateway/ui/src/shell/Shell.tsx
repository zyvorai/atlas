// Copyright (c) 2026 ZyvorAI Labs Private Limited. All rights reserved.
// The macOS-desktop shell: top menu bar + left sidebar + content canvas + bottom dock.
import { useEffect, useMemo, useState } from "react";
import { NavLink, Outlet, useLocation, useNavigate } from "react-router-dom";
import {
  Bell, ChevronLeft, ChevronRight, HardDrive as HardDriveIcon, Hexagon, KeyRound, Loader2, LogOut,
  Moon, Search, Sparkles, Wifi,
} from "lucide-react";
import { MODULES, SECTIONS } from "../nav/modules";
import { http } from "../api/client";
import { useAlerts, useClusters, useJobs } from "../api/hooks";
import { useUi } from "../store/ui";
import { cx, healthKind, stateKind } from "../lib/format";
import { Badge, Button, Field, Modal } from "../ui/kit";

function Clock() {
  const [t, setT] = useState(new Date());
  useEffect(() => {
    const id = setInterval(() => setT(new Date()), 1000);
    return () => clearInterval(id);
  }, []);
  return <span className="tabular-nums">{t.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" })}</span>;
}

function MenuBar({ onSpotlight }: { onSpotlight: () => void }) {
  const { data: clusters } = useClusters();
  const { data: jobs } = useJobs();
  const { data: openAlerts } = useAlerts("open");
  const runningJobs = (jobs || []).filter((j) => ["running", "queued", "verifying", "pending"].includes(j.state));
  const recentDone = (jobs || []).filter((j) => ["succeeded", "failed"].includes(j.state)).slice(0, 5);
  const h = clusters?.[0]?.health || "unknown";
  const [tokenOpen, setTokenOpen] = useState(false);
  const [jobsOpen, setJobsOpen] = useState(false);
  const [bellOpen, setBellOpen] = useState(false);
  const token = useUi((s) => s.token);
  const setToken = useUi((s) => s.setToken);
  const theme = useUi((s) => s.theme);
  const setTheme = useUi((s) => s.setTheme);
  const [draft, setDraft] = useState(token);
  const nav = useNavigate();
  return (
    <div className="h-9 shrink-0 flex items-center gap-3 px-3 glass border-b border-white/[0.06] text-[13px]">
      <div className="flex items-center gap-1.5 font-bold">
        <Hexagon size={15} className="text-sky-400" fill="currentColor" />
        <span className="bg-gradient-to-r from-sky-400 to-blue-500 bg-clip-text text-transparent">ATLAS</span>
      </div>
      <span className="text-muted-foreground/70 hidden sm:inline">Storage Center</span>
      <div className="flex-1" />
      <button className="btn btn-ghost btn-sm gap-1.5" onClick={onSpotlight}>
        <Search size={13} /> <span className="hidden md:inline">Search</span>
        <kbd className="hidden md:inline text-[10px] px-1 rounded bg-white/10">⌘K</kbd>
      </button>
      {runningJobs.length > 0 && (
        <div className="relative">
          <button className="flex items-center gap-1.5 text-xs text-sky-300 btn btn-ghost btn-sm" onClick={() => setJobsOpen((v) => !v)}>
            <Loader2 size={13} className="animate-spin" /> {runningJobs.length} running
          </button>
          {jobsOpen && (
            <>
              <div className="fixed inset-0 z-40" onClick={() => setJobsOpen(false)} />
              <div className="absolute right-0 top-9 z-50 w-72 glass-card p-2 animate-fade-in">
                <div className="section-label px-1.5 pb-1.5">Running jobs</div>
                {runningJobs.slice(0, 8).map((j) => (
                  <button key={j.id} className="w-full text-left px-2 py-1.5 rounded-lg hover:bg-white/[0.06]" onClick={() => { setJobsOpen(false); nav("/jobs"); }}>
                    <div className="flex items-center gap-2">
                      <span className="flex-1 truncate mono text-xs">{j.job_type}</span>
                      <Badge kind={stateKind(j.state)}>{j.state}</Badge>
                    </div>
                    <div className="w-full h-1 rounded-full bg-white/5 mt-1 overflow-hidden">
                      <div className="h-full rounded-full bg-sky-400" style={{ width: `${j.progress_percent || 0}%` }} />
                    </div>
                  </button>
                ))}
              </div>
            </>
          )}
        </div>
      )}
      <div className="relative">
        <button className="btn btn-ghost btn-sm relative" title="Notifications" onClick={() => setBellOpen((v) => !v)}>
          <Bell size={13} />
          {(openAlerts?.length || 0) > 0 && (
            <span className="absolute -top-0.5 -right-0.5 min-w-[14px] h-[14px] px-0.5 rounded-full bg-danger text-white text-[9px] grid place-items-center">{openAlerts!.length}</span>
          )}
        </button>
        {bellOpen && (
          <>
            <div className="fixed inset-0 z-40" onClick={() => setBellOpen(false)} />
            <div className="absolute right-0 top-9 z-50 w-80 glass-card p-2 animate-fade-in max-h-[70vh] overflow-auto">
              <div className="section-label px-1.5 pb-1">Open alerts</div>
              {openAlerts?.length ? openAlerts.slice(0, 6).map((a) => (
                <button key={a.id} className="w-full text-left px-2 py-1.5 rounded-lg hover:bg-white/[0.06]" onClick={() => { setBellOpen(false); nav("/alerts"); }}>
                  <div className="flex items-center gap-2">
                    <Badge kind={stateKind(a.severity)}>{a.severity}</Badge>
                    <span className="flex-1 truncate text-xs">{a.title}</span>
                  </div>
                </button>
              )) : <div className="px-2 py-2 text-xs text-muted-foreground">No open alerts.</div>}
              <div className="section-label px-1.5 pt-2 pb-1">Recent jobs</div>
              {recentDone.length ? recentDone.map((j) => (
                <button key={j.id} className="w-full text-left px-2 py-1.5 rounded-lg hover:bg-white/[0.06]" onClick={() => { setBellOpen(false); nav("/jobs"); }}>
                  <div className="flex items-center gap-2">
                    <Badge kind={stateKind(j.state)}>{j.state}</Badge>
                    <span className="flex-1 truncate mono text-[11px]">{j.job_type}</span>
                  </div>
                </button>
              )) : <div className="px-2 py-2 text-xs text-muted-foreground">No recent jobs.</div>}
            </div>
          </>
        )}
      </div>
      <button
        className="btn btn-ghost btn-sm"
        title={theme === "aurora" ? "Switch to dark" : "Switch to Aurora"}
        onClick={() => setTheme(theme === "aurora" ? "dark" : "aurora")}
      >
        {theme === "aurora" ? <Sparkles size={13} className="text-cyan-300" /> : <Moon size={13} />}
      </button>
      <Badge kind={healthKind(h)} dot>
        {h}
      </Badge>
      <button className={cx("btn btn-ghost btn-sm", token && "text-success")} onClick={() => setTokenOpen(true)} title="Auth token">
        <KeyRound size={13} />
      </button>
      <span className="text-muted-foreground flex items-center gap-1.5">
        <Wifi size={13} className="text-success" />
        <Clock />
      </span>
      <button className="btn btn-ghost btn-sm" title="Sign out" onClick={() => useUi.getState().signOut()}>
        <LogOut size={13} />
      </button>
      <Modal
        open={tokenOpen}
        onClose={() => setTokenOpen(false)}
        title="Service-account token"
        footer={
          <>
            <Button onClick={() => { setToken(""); setDraft(""); setTokenOpen(false); }}>Clear</Button>
            <Button variant="primary" onClick={() => { setToken(draft.trim()); setTokenOpen(false); }}>Save</Button>
          </>
        }
      >
        <div className="text-sm text-muted-foreground mb-2">
          Paste a JWT to authenticate when the gateway has <code className="mono">ATLAS_AUTH_REQUIRED=1</code>. Stored
          locally; sent as a Bearer header.
        </div>
        <Field placeholder="eyJhbGciOi…" value={draft} onChange={(e) => setDraft(e.target.value)} />
      </Modal>
    </div>
  );
}

function Sidebar() {
  const collapsed = useUi((s) => s.sidebarCollapsed);
  const toggle = useUi((s) => s.toggleSidebar);
  const [filter, setFilter] = useState("");
  const [ver, setVer] = useState("");
  useEffect(() => {
    fetch("/version").then((r) => r.json()).then((v) => setVer(v.version)).catch(() => {});
  }, []);
  const grouped = useMemo(() => {
    const f = filter.toLowerCase();
    return SECTIONS.map((sec) => ({
      sec,
      items: MODULES.filter((m) => m.section === sec && (!f || m.label.toLowerCase().includes(f) || m.codename.includes(f))),
    })).filter((g) => g.items.length);
  }, [filter]);
  return (
    <aside className={cx("zeus-sidebar shrink-0 flex flex-col transition-all", collapsed ? "w-[60px]" : "w-[214px]")}>
      {!collapsed && (
        <div className="p-2.5">
          <div className="relative">
            <Search size={13} className="absolute left-2.5 top-1/2 -translate-y-1/2 text-muted-foreground/60" />
            <input
              className="field pl-8 py-1.5 text-xs"
              placeholder="Filter navigation…"
              value={filter}
              onChange={(e) => setFilter(e.target.value)}
            />
          </div>
        </div>
      )}
      <nav className="flex-1 overflow-y-auto px-2 pb-2">
        {grouped.map((g) => (
          <div key={g.sec} className="mb-3">
            {!collapsed && <div className="section-label px-2 mb-1">{g.sec}</div>}
            {g.items.map((m) => (
              <NavLink
                key={m.id}
                to={m.path}
                end={m.path === "/"}
                className={({ isActive }) =>
                  cx(
                    "flex items-center gap-2.5 px-2.5 py-2 rounded-lg text-[13px] font-medium mb-0.5 transition",
                    isActive
                      ? "bg-white/[0.08] text-white shadow-[inset_3px_0_0_theme(colors.sky.400)]"
                      : "text-muted-foreground hover:bg-white/[0.05] hover:text-white",
                    collapsed && "justify-center px-0",
                  )
                }
                title={m.label}
              >
                <m.icon size={16} />
                {!collapsed && <span>{m.label}</span>}
              </NavLink>
            ))}
          </div>
        ))}
      </nav>
      {!collapsed && (
        <div className="px-3 pb-1">
          <a href="https://zyvor.dev" target="_blank" rel="noreferrer" className="flex items-center gap-1.5 text-[11px] text-muted-foreground/60 hover:text-sky-300 transition">
            <Hexagon size={11} className="text-sky-400" fill="currentColor" />
            <span className="flex-1">Zyvor · Atlas{ver && ` v${ver}`}</span>
          </a>
        </div>
      )}
      <button className="btn btn-ghost btn-sm m-2 justify-center" onClick={toggle}>
        {collapsed ? <ChevronRight size={14} /> : <ChevronLeft size={14} />}
      </button>
    </aside>
  );
}

function Dock() {
  const dockMods = MODULES.filter((m) => m.dock);
  const nav = useNavigate();
  const loc = useLocation();
  return (
    <div className="shrink-0 flex justify-center pb-2 pt-1">
      <div className="glass rounded-2xl px-2 py-1.5 flex items-center gap-1.5 shadow-[0_10px_40px_-10px_rgba(0,0,0,.7)]">
        {dockMods.map((m) => {
          const active = m.path === "/" ? loc.pathname === "/" : loc.pathname.startsWith(m.path);
          return (
            <button
              key={m.id}
              onClick={() => nav(m.path)}
              title={m.label}
              className={cx(
                "w-9 h-9 rounded-xl grid place-items-center transition hover:-translate-y-1",
                active ? "bg-gradient-to-br from-sky-400 to-blue-600 text-white" : "bg-white/[0.06] text-muted-foreground hover:text-white",
              )}
            >
              <m.icon size={17} />
            </button>
          );
        })}
      </div>
    </div>
  );
}

interface SpotItem {
  label: string;
  sub: string;
  path: string;
  icon: (typeof MODULES)[number]["icon"];
}

function Spotlight({ open, onClose }: { open: boolean; onClose: () => void }) {
  const nav = useNavigate();
  const [q, setQ] = useState("");
  const [sel, setSel] = useState(0);
  const [resources, setResources] = useState<SpotItem[]>([]);
  useEffect(() => { if (open) { setQ(""); setSel(0); } }, [open]);
  useEffect(() => { setSel(0); }, [q]);
  // Fetch resources once per open so ⌘K can jump straight to a volume/bucket/snapshot/backup.
  useEffect(() => {
    if (!open) return;
    Promise.allSettled([http.get("/volumes"), http.get("/buckets"), http.get("/snapshots"), http.get("/backups")]).then(
      ([v, b, s, bk]) => {
        const out: SpotItem[] = [];
        if (v.status === "fulfilled") v.value.data.forEach((x: any) => out.push({ label: x.name, sub: "volume", path: `/volumes?focus=${x.id}`, icon: HardDriveIcon }));
        if (b.status === "fulfilled") b.value.data.forEach((x: any) => out.push({ label: x.bucket_name || x.id, sub: "bucket", path: "/buckets", icon: HardDriveIcon }));
        if (s.status === "fulfilled") s.value.data.forEach((x: any) => out.push({ label: x.name, sub: "snapshot", path: "/snapshots", icon: HardDriveIcon }));
        if (bk.status === "fulfilled") bk.value.data.forEach((x: any) => out.push({ label: x.id, sub: "backup", path: "/backups", icon: HardDriveIcon }));
        setResources(out);
      },
    );
  }, [open]);

  const modItems: SpotItem[] = MODULES.map((m) => ({ label: m.label, sub: m.section.toLowerCase(), path: m.path, icon: m.icon }));
  const ql = q.toLowerCase();
  const items = [...modItems, ...resources].filter((it) => !q || it.label.toLowerCase().includes(ql) || it.sub.includes(ql)).slice(0, 40);
  if (!open) return null;
  const go = (i: number) => { const it = items[i]; if (it) { nav(it.path); onClose(); } };
  return (
    <div className="fixed inset-0 z-[70] pt-[14vh] px-4 flex justify-center" onMouseDown={onClose}>
      <div className="absolute inset-0 bg-black/50 backdrop-blur-sm" />
      <div className="relative glass-card w-[560px] max-w-full overflow-hidden animate-fade-in" onMouseDown={(e) => e.stopPropagation()}>
        <div className="flex items-center gap-2 px-4 py-3 border-b border-white/[0.06]">
          <Search size={16} className="text-muted-foreground" />
          <input
            autoFocus
            className="bg-transparent outline-none flex-1 text-sm"
            placeholder="Search Centers, volumes, buckets, snapshots, backups…"
            value={q}
            onChange={(e) => setQ(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "ArrowDown") { e.preventDefault(); setSel((i) => Math.min(items.length - 1, i + 1)); }
              else if (e.key === "ArrowUp") { e.preventDefault(); setSel((i) => Math.max(0, i - 1)); }
              else if (e.key === "Enter") go(sel);
              else if (e.key === "Escape") onClose();
            }}
          />
        </div>
        <div className="max-h-[46vh] overflow-auto p-1.5">
          {items.map((it, i) => (
            <button
              key={`${it.path}-${it.label}-${i}`}
              onMouseEnter={() => setSel(i)}
              onClick={() => go(i)}
              className={cx("w-full flex items-center gap-3 px-3 py-2.5 rounded-lg text-left", i === sel ? "bg-white/[0.08]" : "hover:bg-white/[0.06]")}
            >
              <it.icon size={16} className="text-sky-400" />
              <span className="flex-1 text-sm truncate">{it.label}</span>
              <span className="text-[11px] text-muted-foreground/60">{it.sub}</span>
            </button>
          ))}
          {!items.length && <div className="px-3 py-6 text-center text-sm text-muted-foreground">No matches.</div>}
        </div>
      </div>
    </div>
  );
}

let welcomed = false;

const SHORTCUTS: [string, string][] = [
  ["⌘K / Ctrl-K", "Open spotlight search"],
  ["↑ ↓ / Enter", "Navigate & open in spotlight"],
  ["?", "Show this help"],
  ["Esc", "Close dialogs, drawers, menus"],
];

export function Shell() {
  const spotOpen = useUi((s) => s.spotlightOpen);
  const setSpot = useUi((s) => s.setSpotlight);
  const [helpOpen, setHelpOpen] = useState(false);
  const loc = useLocation();
  useEffect(() => {
    const h = (e: KeyboardEvent) => {
      const typing = ["INPUT", "TEXTAREA", "SELECT"].includes((e.target as HTMLElement)?.tagName);
      if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "k") {
        e.preventDefault();
        setSpot(true);
      } else if (e.key === "?" && !typing) {
        e.preventDefault();
        setHelpOpen(true);
      }
    };
    window.addEventListener("keydown", h);
    return () => window.removeEventListener("keydown", h);
  }, [setSpot]);
  // Per-page document title.
  useEffect(() => {
    const m = MODULES.find((x) => (x.path === "/" ? loc.pathname === "/" : loc.pathname.startsWith(x.path)));
    document.title = m ? `Atlas · ${m.label}` : "Atlas — Storage Center";
  }, [loc.pathname]);
  // Welcome once per session; auto-collapse sidebar on narrow viewports.
  useEffect(() => {
    if (!welcomed) {
      welcomed = true;
      import("../api/client").then((c) => c.toast("Welcome to Atlas Storage Center", "ok"));
    }
    const onResize = () => {
      if (window.innerWidth < 900 && !useUi.getState().sidebarCollapsed) useUi.getState().toggleSidebar();
    };
    onResize();
    window.addEventListener("resize", onResize);
    return () => window.removeEventListener("resize", onResize);
  }, []);
  return (
    <div className="h-full flex flex-col overflow-hidden">
      <MenuBar onSpotlight={() => setSpot(true)} />
      <div className="flex-1 flex min-h-0">
        <Sidebar />
        <main className="tahoe-canvas flex-1 min-w-0 flex flex-col">
          <div key={loc.pathname} className="flex-1 overflow-auto p-6 animate-fade-in">
            <Outlet />
          </div>
          <Dock />
        </main>
      </div>
      <Spotlight open={spotOpen} onClose={() => setSpot(false)} />
      <Modal open={helpOpen} onClose={() => setHelpOpen(false)} title="Keyboard shortcuts">
        <div className="space-y-2">
          {SHORTCUTS.map(([k, d]) => (
            <div key={k} className="flex items-center justify-between text-sm">
              <span className="text-muted-foreground">{d}</span>
              <kbd className="px-2 py-0.5 rounded bg-white/10 text-xs mono">{k}</kbd>
            </div>
          ))}
        </div>
      </Modal>
    </div>
  );
}
