// Copyright (c) 2026 ZyvorAI Labs Private Limited. All rights reserved.
// Soundings shell: chart floor + slim topbar (branding/search/status) + collapsible sidebar + canvas.
import { useEffect, useMemo, useState } from "react";
import { createPortal } from "react-dom";
import { NavLink, Outlet, useLocation, useNavigate } from "react-router-dom";
import {
  Bell,
  ChevronLeft,
  ChevronRight,
  HardDrive as HardDriveIcon,
  KeyRound,
  LayoutTemplate,
  Loader2,
  LogOut,
  Menu,
  Pause,
  Play,
  Search,
  X,
} from "lucide-react";
import { MODULES, SECTIONS, MENUBAR_CONTROLS } from "../nav/modules";
import { http, isUnauthorized } from "../api/client";
import { useAlerts, useCephHealthRollup, useClusters, useJobs } from "../api/hooks";
import { useUi } from "../store/ui";
import { THEME_OPTIONS, themeTitle } from "../lib/themes";
import { cx } from "../lib/format";
import { onSendPrompt } from "../lib/prompts";
import { Button, Field, Modal } from "../ui/kit";
import { ChartFloor } from "../ui/ChartFloor";
import LicenseBanner from "../components/LicenseBanner";

function Clock() {
  const [t, setT] = useState(new Date());
  useEffect(() => {
    const id = setInterval(() => setT(new Date()), 1000);
    return () => clearInterval(id);
  }, []);
  return (
    <span className="mono" style={{ fontSize: 12, color: "var(--at-ink-3)" }}>
      {t.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" })}
    </span>
  );
}

function MenuBar({
  onSpotlight,
  onOpenNav,
}: {
  onSpotlight: () => void;
  onOpenNav: () => void;
}) {
  const { data: clusters, isError: clustersErrored, error: clustersError } = useClusters();
  const { data: jobs } = useJobs();
  const { data: openAlerts } = useAlerts("open");
  // Prefer Atlas's own 5-value severity rollup (Healthy/Degraded/Rebuilding/At Risk/Critical,
  // synthesized from status/osd-tree/osd-df — see atlas_driver_ceph::health_rollup) over the
  // older 4-value per-cluster Health field; fall back to the latter if the rollup call errors.
  const { data: rollup, isError: rollupErrored } = useCephHealthRollup();
  const runningJobs = (jobs || []).filter((j) =>
    ["running", "queued", "verifying", "pending"].includes(j.state),
  );
  // Only a real 401 is AUTH. Connection refused / probe flaps / 5xx used to be mislabeled as
  // "API requests rejected — check your session" while live metrics still worked from cache.
  const authFailed = clustersErrored && isUnauthorized(clustersError);
  const unreachable = clustersErrored && !authFailed && !clusters;
  const h = authFailed
    ? "unauthenticated"
    : unreachable
      ? "unreachable"
      : !rollupErrored && rollup?.state
        ? rollup.state
        : clusters?.[0]?.health || "unknown";
  const healthWhy = useMemo(() => {
    if (h === "unauthenticated") return "API requests rejected — check your session";
    if (h === "unreachable") return "Gateway unreachable — retrying";
    if (h === "ok" || h === "healthy" || h === "unknown") return undefined;
    return (
      rollup?.summary ||
      openAlerts?.[0]?.title ||
      openAlerts?.[0]?.description ||
      (h === "critical" ? "Cluster health critical" : "Cluster health degraded")
    );
  }, [h, openAlerts, rollup]);
  const healthClass =
    h === "ok" || h === "healthy"
      ? "ok"
      : h === "warn" || h === "degraded"
        ? "warn"
        : h === "rebuilding"
          ? "info"
          : h === "at_risk"
            ? "at-risk"
            : h === "critical" || h === "unauthenticated"
              ? "crit"
              : "muted";
  const healthLabel =
    h === "ok"
      ? "HEALTH_OK"
      : h === "healthy"
        ? "HEALTHY"
        : h === "warn"
          ? "HEALTH_WARN"
          : h === "degraded"
            ? "DEGRADED"
            : h === "rebuilding"
              ? "REBUILDING"
              : h === "at_risk"
                ? "AT RISK"
                : h === "critical"
                  ? "HEALTH_ERR"
                  : h === "unauthenticated"
                    ? "AUTH"
                    : h === "unreachable"
                      ? "UNREACHABLE"
                      : String(h).toUpperCase();

  const [tokenOpen, setTokenOpen] = useState(false);
  const [jobsOpen, setJobsOpen] = useState(false);
  const [bellOpen, setBellOpen] = useState(false);
  const [themeOpen, setThemeOpen] = useState(false);
  const [acctOpen, setAcctOpen] = useState(false);
  const token = useUi((s) => s.token);
  const setToken = useUi((s) => s.setToken);
  const theme = useUi((s) => s.theme);
  const setTheme = useUi((s) => s.setTheme);
  const paused = useUi((s) => s.paused);
  const togglePaused = useUi((s) => s.togglePaused);
  const [draft, setDraft] = useState(token);
  useEffect(() => {
    if (tokenOpen) setDraft(token);
  }, [tokenOpen, token]);
  const nav = useNavigate();

  return (
    <header className="at-rail">
      <div className="at-rail-brand">
        <button
          type="button"
          className="at-iconbtn at-menu-burger"
          title="Open navigation"
          aria-label="Open navigation"
          onClick={onOpenNav}
        >
          <Menu size={16} strokeWidth={2} />
        </button>

        <NavLink to="/" end className="at-mark" title="Atlas — Command Deck" aria-label="Atlas — Command Deck">
          <span className="at-brand-logo" aria-hidden>
            <svg width="18" height="18" viewBox="0 0 24 24" fill="none">
              <circle cx="12" cy="12" r="3" fill="currentColor" />
              <circle cx="12" cy="12" r="6.5" stroke="currentColor" strokeWidth="1.3" opacity=".62" />
              <circle cx="12" cy="12" r="10" stroke="currentColor" strokeWidth="1.1" opacity=".3" />
            </svg>
          </span>
        </NavLink>
      </div>

      <div className="at-rail-actions">
        <div className="at-rail-tray">
          <button
            type="button"
            className="at-iconbtn"
            title="Search — ⌘K / Ctrl+K"
            aria-label="Search pools, volumes, hosts"
            onClick={onSpotlight}
          >
            <Search size={16} strokeWidth={2} />
          </button>

          <div className="relative">
            <button
              type="button"
              className="at-iconbtn at-look-btn"
              title={`Look & feel: ${themeTitle(theme)}`}
              aria-label={`Look & feel — ${themeTitle(theme)}. Choose Carbon or Apple Lite.`}
              aria-expanded={themeOpen}
              aria-haspopup="menu"
              onClick={() => setThemeOpen((v) => !v)}
            >
              <LayoutTemplate size={16} strokeWidth={2} />
            </button>
            {themeOpen && (
              <>
                <div className="fixed inset-0 z-40" onClick={() => setThemeOpen(false)} />
                <div className="at-theme-menu" role="menu" aria-label="Look and feel">
                  <div className="at-theme-menu-label">Look &amp; feel</div>
                  {THEME_OPTIONS.map((opt) => (
                    <button
                      key={opt.id}
                      type="button"
                      role="menuitemradio"
                      aria-checked={theme === opt.id}
                      className={cx("at-theme-item", theme === opt.id && "on")}
                      onClick={() => {
                        setTheme(opt.id);
                        setThemeOpen(false);
                      }}
                    >
                      <span>{opt.title}</span>
                      <span className="hint">{opt.hint}</span>
                    </button>
                  ))}
                </div>
              </>
            )}
          </div>

          {runningJobs.length > 0 && (
            <div className="relative">
              <button
                type="button"
                className="at-iconbtn"
                title={`${runningJobs.length} running jobs`}
                onClick={() => setJobsOpen((v) => !v)}
              >
                <Loader2 size={15} className="animate-spin" style={{ color: "var(--at-cyan)" }} />
              </button>
              {jobsOpen && (
                <>
                  <div className="fixed inset-0 z-40" onClick={() => setJobsOpen(false)} />
                  <div className="at-theme-menu at-theme-menu-wide">
                    <div className="at-theme-menu-label">Running jobs</div>
                    {runningJobs.slice(0, 8).map((j) => (
                      <button
                        key={j.id}
                        type="button"
                        className="at-theme-item"
                        onClick={() => {
                          setJobsOpen(false);
                          nav("/jobs");
                        }}
                      >
                        <span className="mono" style={{ fontSize: 12 }}>
                          {j.job_type}
                        </span>
                        <span className="hint">{j.state}</span>
                      </button>
                    ))}
                  </div>
                </>
              )}
            </div>
          )}

          <div className="relative">
            <button type="button" className="at-iconbtn" title="Alerts" onClick={() => setBellOpen((v) => !v)}>
              <Bell size={16} />
              {(openAlerts?.length || 0) > 0 && <span className="at-badge">{openAlerts!.length}</span>}
            </button>
            {bellOpen && (
              <>
                <div className="fixed inset-0 z-40" onClick={() => setBellOpen(false)} />
                <div className="at-theme-menu at-theme-menu-wide">
                  <div className="at-theme-menu-label">Open alerts</div>
                  {openAlerts?.length ? (
                    openAlerts.slice(0, 6).map((a) => (
                      <button
                        key={a.id}
                        type="button"
                        className="at-theme-item"
                        onClick={() => {
                          setBellOpen(false);
                          nav("/alerts");
                        }}
                      >
                        <span className="truncate">{a.title}</span>
                        <span className="hint">{a.severity}</span>
                      </button>
                    ))
                  ) : (
                    <div className="at-theme-item" style={{ cursor: "default" }}>
                      <span className="hint">No open alerts.</span>
                    </div>
                  )}
                </div>
              </>
            )}
          </div>

          <button
            type="button"
            className={cx("at-iconbtn", paused && "is-on")}
            title={paused ? "Resume auto-refresh" : "Pause live telemetry"}
            onClick={togglePaused}
          >
            {paused ? <Play size={15} /> : <Pause size={15} />}
          </button>

          <div className={cx("at-health", healthClass)} title={healthWhy}>
            <span className="dot" />
            <span>{healthLabel}</span>
          </div>

          <span className="at-rail-sep" aria-hidden />

          <div className="relative">
            <button
              type="button"
              className="at-iconbtn"
              title="Account"
              aria-label="Account"
              aria-expanded={acctOpen}
              aria-haspopup="menu"
              onClick={() => setAcctOpen((v) => !v)}
              style={token ? { color: "var(--at-ok)" } : undefined}
            >
              <KeyRound size={15} />
            </button>
            {acctOpen && (
              <>
                <div className="fixed inset-0 z-40" onClick={() => setAcctOpen(false)} />
                <div className="at-theme-menu" role="menu" aria-label="Account">
                  <div className="at-theme-menu-label">Account</div>
                  <div className="at-theme-item" style={{ cursor: "default" }}>
                    <span className="hint">Local time</span>
                    <Clock />
                  </div>
                  <button
                    type="button"
                    className="at-theme-item"
                    onClick={() => {
                      setAcctOpen(false);
                      setTokenOpen(true);
                    }}
                  >
                    <span>Auth token</span>
                    <span className="hint">{token ? "Set" : "Not set"}</span>
                  </button>
                  <button
                    type="button"
                    className="at-theme-item"
                    onClick={() => useUi.getState().signOut()}
                  >
                    <span className="at-controls-menu-row">
                      <LogOut size={13} strokeWidth={2} aria-hidden />
                      <span>Sign out</span>
                    </span>
                  </button>
                </div>
              </>
            )}
          </div>
        </div>
      </div>

      <Modal
        open={tokenOpen}
        onClose={() => setTokenOpen(false)}
        title="Service-account token"
        footer={
          <>
            <Button
              onClick={() => {
                setToken("");
                setDraft("");
                setTokenOpen(false);
              }}
            >
              Clear
            </Button>
            <Button
              variant="primary"
              onClick={() => {
                setToken(draft.trim());
                setTokenOpen(false);
              }}
            >
              Save
            </Button>
          </>
        }
      >
        <div className="text-sm text-muted-foreground mb-2">
          Optional JWT override. Console login already mints a session token.
        </div>
        <Field
          type="password"
          autoComplete="off"
          placeholder="eyJhbGciOi…"
          value={draft}
          onChange={(e) => setDraft(e.target.value)}
        />
      </Modal>
    </header>
  );
}

const SECTION_SHORT: Record<string, string> = {
  STORAGE: "Storage",
  "DATA PROTECTION": "Protect",
  DATABRIDGE: "DataBridge",
  OBSERVABILITY: "Observe",
  GOVERNANCE: "Govern",
  INFRASTRUCTURE: "Infra",
};

/**
 * Persistent left navigation — Zeus OS's sidebar pattern: the six section groups stay always
 * visible, collapsible down to an icon-only rail. The topbar above carries only branding,
 * search, and status, so navigation lives in exactly one place instead of duplicating a
 * top-bar mega-menu and a sidebar.
 */
function Sidebar({ collapsed, onToggle }: { collapsed: boolean; onToggle: () => void }) {
  const grouped = useMemo(
    () =>
      SECTIONS.map((sec) => ({
        sec,
        short: SECTION_SHORT[sec] || sec,
        items: MODULES.filter((m) => m.section === sec),
      })).filter((g) => g.items.length),
    [],
  );

  return (
    <aside className={cx("at-sidebar", collapsed && "collapsed")} aria-label="Primary">
      <nav className="at-sidebar-body">
        {grouped.map((g) => (
          <div key={g.sec} className="at-sidebar-group">
            {!collapsed && <div className="at-sidebar-label">{g.short}</div>}
            {g.items.map((m) => (
              <NavLink
                key={m.id}
                to={m.path}
                end={m.path === "/"}
                title={collapsed ? m.label : undefined}
                aria-label={m.label}
                className={({ isActive }) => cx("at-sidebar-link", isActive && "on")}
              >
                <m.icon strokeWidth={2} aria-hidden />
                {!collapsed && <span>{m.label}</span>}
              </NavLink>
            ))}
          </div>
        ))}
      </nav>
      <button
        type="button"
        className="at-sidebar-toggle"
        title={collapsed ? "Expand sidebar" : "Collapse sidebar"}
        aria-label={collapsed ? "Expand sidebar" : "Collapse sidebar"}
        onClick={onToggle}
      >
        {collapsed ? <ChevronRight size={14} strokeWidth={2} /> : <ChevronLeft size={14} strokeWidth={2} />}
      </button>
    </aside>
  );
}

function MobileNavDrawer({ open, onClose }: { open: boolean; onClose: () => void }) {
  const loc = useLocation();
  useEffect(() => {
    onClose();
  }, [loc.pathname]); // eslint-disable-line react-hooks/exhaustive-deps
  useEffect(() => {
    if (!open) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [open, onClose]);

  if (!open) return null;

  const grouped = SECTIONS.map((sec) => ({
    sec,
    short: SECTION_SHORT[sec] || sec,
    items: MODULES.filter((m) => m.section === sec),
  })).filter((g) => g.items.length);

  return createPortal(
    <div className="at-drawer-scrim" onMouseDown={onClose}>
      <aside
        className="at-drawer"
        role="dialog"
        aria-modal="true"
        aria-label="Navigation"
        onMouseDown={(e) => e.stopPropagation()}
      >
        <div className="at-drawer-head">
          <span className="at-drawer-title">Navigate</span>
          <button type="button" className="at-iconbtn" aria-label="Close navigation" onClick={onClose}>
            <X size={16} />
          </button>
        </div>
        <div className="at-drawer-body">
          <div className="at-drawer-label">Shortcuts</div>
          {MENUBAR_CONTROLS.map((m) => (
            <NavLink
              key={m.id}
              to={m.path}
              end={m.path === "/"}
              className={({ isActive }) => cx("at-drawer-item", isActive && "on")}
              onClick={onClose}
            >
              <m.icon size={16} strokeWidth={2} aria-hidden />
              <span className="grow">{m.label}</span>
              {m.shortcut ? <kbd className="at-control-kbd">{m.shortcut}</kbd> : null}
            </NavLink>
          ))}
          {grouped.map((g) => (
            <div key={g.sec}>
              <div className="at-drawer-label">{g.short}</div>
              {g.items.map((m) => (
                <NavLink
                  key={m.id}
                  to={m.path}
                  end={m.path === "/"}
                  className={({ isActive }) => cx("at-drawer-item", isActive && "on")}
                  onClick={onClose}
                >
                  <m.icon size={16} strokeWidth={2} aria-hidden />
                  <span>{m.label}</span>
                </NavLink>
              ))}
            </div>
          ))}
        </div>
      </aside>
    </div>,
    document.body,
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
  useEffect(() => {
    if (open) {
      setQ("");
      setSel(0);
    }
  }, [open]);
  useEffect(() => {
    setSel(0);
  }, [q]);
  useEffect(() => {
    if (!open) return;
    Promise.allSettled([
      http.get("/volumes"),
      http.get("/buckets"),
      http.get("/snapshots"),
      http.get("/backups"),
    ]).then(([v, b, s, bk]) => {
      const out: SpotItem[] = [];
      if (v.status === "fulfilled")
        v.value.data.forEach((x: { name: string; id: string }) =>
          out.push({ label: x.name, sub: "volume", path: `/volumes?focus=${x.id}`, icon: HardDriveIcon }),
        );
      if (b.status === "fulfilled")
        b.value.data.forEach((x: { bucket_name?: string; id: string }) =>
          out.push({
            label: x.bucket_name || x.id,
            sub: "bucket",
            path: "/buckets",
            icon: HardDriveIcon,
          }),
        );
      if (s.status === "fulfilled")
        s.value.data.forEach((x: { name: string }) =>
          out.push({ label: x.name, sub: "snapshot", path: "/snapshots", icon: HardDriveIcon }),
        );
      if (bk.status === "fulfilled")
        bk.value.data.forEach((x: { id: string }) =>
          out.push({ label: x.id, sub: "backup", path: "/backups", icon: HardDriveIcon }),
        );
      setResources(out);
    });
  }, [open]);

  const modItems: SpotItem[] = MODULES.map((m) => ({
    label: m.label,
    sub: m.section.toLowerCase(),
    path: m.path,
    icon: m.icon,
  }));
  const ql = q.toLowerCase();
  const items = [...modItems, ...resources]
    .filter((it) => !q || it.label.toLowerCase().includes(ql) || it.sub.includes(ql))
    .slice(0, 40);
  if (!open) return null;
  const close = () => {
    setQ("");
    setSel(0);
    onClose();
  };
  const go = (i: number) => {
    const it = items[i];
    if (it) {
      nav(it.path);
      close();
    }
  };
  return (
    <div className="at-scrim" onMouseDown={close}>
      <div className="at-palette" role="dialog" aria-label="Command palette" onMouseDown={(e) => e.stopPropagation()}>
        <input
          autoFocus
          className="at-palette-input"
          placeholder="Go to, create, or ask…"
          value={q}
          onChange={(e) => setQ(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "ArrowDown") {
              e.preventDefault();
              setSel((i) => Math.min(items.length - 1, i + 1));
            } else if (e.key === "ArrowUp") {
              e.preventDefault();
              setSel((i) => Math.max(0, i - 1));
            } else if (e.key === "Enter") go(sel);
            else if (e.key === "Escape") close();
          }}
        />
        <div style={{ maxHeight: "46vh", overflow: "auto", padding: "6px 0" }}>
          {items.map((it, i) => (
            <button
              key={`${it.path}-${it.label}-${i}`}
              type="button"
              className={cx("at-pitem", i === sel && "sel")}
              onMouseEnter={() => setSel(i)}
              onClick={() => go(i)}
            >
              <it.icon size={16} style={{ color: "var(--at-cyan)", opacity: 0.85 }} />
              <span className="flex-1 truncate">{it.label}</span>
              <span className="k">{it.sub}</span>
            </button>
          ))}
          {!items.length && (
            <div style={{ padding: "24px", textAlign: "center", color: "var(--at-ink-4)", fontSize: 13 }}>
              No matches.
            </div>
          )}
        </div>
      </div>
    </div>
  );
}

function AgentToast() {
  const [q, setQ] = useState<string | null>(null);
  useEffect(() => {
    let t: ReturnType<typeof setTimeout>;
    return onSendPrompt((prompt) => {
      setQ(prompt);
      clearTimeout(t);
      t = setTimeout(() => setQ(null), 3400);
    });
  }, []);
  return (
    <div className={cx("at-toast", q && "on")} role="status">
      <span>Handed to agent:</span>
      <span className="q">{q}</span>
    </div>
  );
}

let welcomed = false;

const SHORTCUTS: [string, string][] = [
  ["H", "Go to Command Deck"],
  ["⌘K / Ctrl-K", "Open command palette"],
  ["↑ ↓ / Enter", "Navigate & open in palette"],
  ["?", "Show this help"],
  ["Esc", "Close dialogs / menus"],
];

export function Shell() {
  const spotOpen = useUi((s) => s.spotlightOpen);
  const setSpot = useUi((s) => s.setSpotlight);
  const sidebarCollapsed = useUi((s) => s.sidebarCollapsed);
  const toggleSidebar = useUi((s) => s.toggleSidebar);
  const [helpOpen, setHelpOpen] = useState(false);
  const [navOpen, setNavOpen] = useState(false);
  const loc = useLocation();
  const nav = useNavigate();
  useEffect(() => {
    const h = (e: KeyboardEvent) => {
      const typing = ["INPUT", "TEXTAREA", "SELECT"].includes((e.target as HTMLElement)?.tagName)
        || (e.target as HTMLElement)?.isContentEditable;
      if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "k") {
        e.preventDefault();
        setSpot(true);
      } else if (e.key === "?" && !typing) {
        e.preventDefault();
        setHelpOpen(true);
      } else if (!typing && !e.metaKey && !e.ctrlKey && !e.altKey && e.key.toLowerCase() === "h") {
        e.preventDefault();
        nav("/");
      }
    };
    window.addEventListener("keydown", h);
    return () => window.removeEventListener("keydown", h);
  }, [setSpot, nav]);
  useEffect(() => {
    // Only set the title for routes with a known static label. Dynamic detail routes
    // (/pools/:id, PlanDetail, …) aren't in MODULES — leave the title alone for those so the
    // view's own effect can name the specific entity once its data loads, instead of this
    // effect re-asserting the generic fallback and racing it (index.html already ships that
    // fallback as the default, so there's nothing to set here on first paint anyway).
    const m = MODULES.find((x) => (x.path === "/" ? loc.pathname === "/" : loc.pathname.startsWith(x.path)));
    if (m) document.title = `Atlas · ${m.label}`;
  }, [loc.pathname]);
  useEffect(() => {
    if (!welcomed) {
      welcomed = true;
      import("../api/client").then((c) => c.toast("Welcome to Atlas Storage Center", "ok"));
    }
  }, []);

  return (
    <div className="at-app h-full flex flex-col overflow-hidden">
      <ChartFloor />
      <MenuBar onSpotlight={() => setSpot(true)} onOpenNav={() => setNavOpen(true)} />
      <LicenseBanner />
      <div className="at-shell-body flex-1 min-h-0">
        <Sidebar collapsed={sidebarCollapsed} onToggle={toggleSidebar} />
        <div className="at-main flex-1 min-h-0">
          <div key={loc.pathname} className="at-main-scroll">
            <Outlet />
          </div>
        </div>
      </div>
      <Spotlight open={spotOpen} onClose={() => setSpot(false)} />
      <MobileNavDrawer open={navOpen} onClose={() => setNavOpen(false)} />
      <AgentToast />
      <Modal open={helpOpen} onClose={() => setHelpOpen(false)} title="Keyboard shortcuts">
        <div className="space-y-2">
          {SHORTCUTS.map(([k, d]) => (
            <div key={k} className="flex items-center justify-between text-sm">
              <span className="text-muted-foreground">{d}</span>
              <kbd className="at-kbd mono" style={{ marginLeft: 0 }}>{k}</kbd>
            </div>
          ))}
        </div>
      </Modal>
    </div>
  );
}
