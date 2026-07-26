// Copyright (c) 2026 ZyvorAI Labs Private Limited. All rights reserved.
// Premium branded login — PacketWolf / Zeus suite split-hero shell, Atlas identity.
import { useEffect, useState, type FormEvent } from "react";
import {
  ArrowRight,
  CheckCircle,
  Database,
  HardDrive,
  Hexagon,
  KeyRound,
  Layers,
  Palette,
  Shield,
  Sparkles,
} from "lucide-react";
import { useUi, type Theme } from "../store/ui";
import {
  LoginField,
  LoginRemember,
  LoginSubmit,
  PremiumLoginShell,
  type PremiumLoginFeature,
} from "../ui/PremiumLoginShell";

const PRODUCT = "Atlas";
const REMEMBER_TOKEN_KEY = "atlas.login-remember-token";
const REMEMBER_FLAG_KEY = "atlas.login-remember";

const FEATURES: PremiumLoginFeature[] = [
  {
    icon: <HardDrive className="w-5 h-5 text-white" aria-hidden />,
    title: "Multi-backend control plane",
    description: "Ceph, NFS, and ZFS behind one StorageDriver API and inventory.",
    gradient: "from-sky-500/95 to-blue-600/95",
    glow: "shadow-sky-500/30",
  },
  {
    icon: <Layers className="w-5 h-5 text-white" aria-hidden />,
    title: "Volumes · snapshots · RGW",
    description: "PVC and RBD lifecycle, clones, CephFS RWX, buckets, and backups.",
    gradient: "from-violet-500/95 to-indigo-600/95",
    glow: "shadow-violet-500/30",
    highlight: true,
  },
  {
    icon: <Database className="w-5 h-5 text-white" aria-hidden />,
    title: "DataBridge migration",
    description: "Cloud-to-edge DB pipelines — discover through cutover on Ceph.",
    gradient: "from-cyan-500/95 to-teal-600/95",
    glow: "shadow-cyan-500/30",
  },
  {
    icon: <Shield className="w-5 h-5 text-white" aria-hidden />,
    title: "Day-2 ops & DR",
    description: "Observatory, alerts, maintenance, governance, and RBD mirror peers.",
    gradient: "from-emerald-500/95 to-green-600/95",
    glow: "shadow-emerald-500/30",
  },
];

const THEME_OPTIONS: { id: Theme; label: string }[] = [
  { id: "nebula", label: "Nebula" },
  { id: "dark", label: "Dark" },
  { id: "aurora", label: "Aurora" },
];

function LoginThemeSwitcher() {
  const theme = useUi((s) => s.theme);
  const setTheme = useUi((s) => s.setTheme);
  const [open, setOpen] = useState(false);

  useEffect(() => {
    if (!open) return;
    const onPointer = (e: PointerEvent) => {
      const el = document.getElementById("atlas-login-theme");
      if (el && !el.contains(e.target as Node)) setOpen(false);
    };
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") setOpen(false);
    };
    document.addEventListener("pointerdown", onPointer);
    document.addEventListener("keydown", onKey);
    return () => {
      document.removeEventListener("pointerdown", onPointer);
      document.removeEventListener("keydown", onKey);
    };
  }, [open]);

  return (
    <div id="atlas-login-theme" className="absolute top-3 right-3 sm:top-4 sm:right-4 z-30">
      <button
        type="button"
        onClick={() => setOpen((v) => !v)}
        title="Change theme"
        aria-label="Change theme"
        aria-expanded={open}
        className="rounded-full p-2 backdrop-blur-xl border border-white/10 bg-black/40 text-muted-foreground hover:text-foreground transition"
      >
        <Palette className="w-4 h-4" aria-hidden />
      </button>
      {open && (
        <div
          className="mt-2 rounded-xl p-1.5 backdrop-blur-xl border border-white/10 bg-black/60 grid grid-cols-3 gap-0.5"
          role="group"
          aria-label="Visual theme"
        >
          {THEME_OPTIONS.map(({ id, label }) => (
            <button
              key={id}
              type="button"
              onClick={() => {
                setTheme(id);
                setOpen(false);
              }}
              className={`rounded-lg border px-2 py-1.5 text-[9px] font-bold uppercase tracking-wide transition ${
                theme === id
                  ? "border-sky-400/60 bg-sky-500/20 text-sky-200"
                  : "border-transparent text-muted-foreground hover:bg-white/5"
              }`}
            >
              {label}
            </button>
          ))}
        </div>
      )}
    </div>
  );
}

function ZyvorFooter() {
  return (
    <footer className="shrink-0 py-2 text-center text-xs text-muted-foreground" role="contentinfo">
      <a
        href="https://zyvor.dev"
        target="_blank"
        rel="noopener noreferrer"
        className="hover:text-sky-300 transition-colors"
      >
        zyvor.dev
      </a>
      <span aria-hidden> · </span>
      <span>Atlas</span>
      <span aria-hidden> · </span>
      <span>© 2026</span>
    </footer>
  );
}

export function Login() {
  const enter = useUi((s) => s.enter);
  const setToken = useUi((s) => s.setToken);
  const [token, setDraft] = useState(() => useUi.getState().token);
  const [remember, setRemember] = useState(false);
  const [ver, setVer] = useState("");
  const [health, setHealth] = useState<"ok" | "down" | "…">("…");
  const hostLabel = typeof window !== "undefined" ? window.location.hostname : "";

  useEffect(() => {
    document.title = `Sign in · ${PRODUCT}`;
    const remembered = localStorage.getItem(REMEMBER_FLAG_KEY) === "true";
    const saved = localStorage.getItem(REMEMBER_TOKEN_KEY);
    if (remembered && saved) {
      setDraft(saved);
      setRemember(true);
    }
    fetch("/version")
      .then((r) => r.json())
      .then((v) => setVer(v.version))
      .catch(() => {});
    fetch("/health")
      .then((r) => setHealth(r.ok ? "ok" : "down"))
      .catch(() => setHealth("down"));
    return () => {
      document.title = PRODUCT;
    };
  }, []);

  const go = (e?: FormEvent) => {
    e?.preventDefault();
    const t = token.trim();
    // `remember` also decides where the *session itself* lives (localStorage vs. sessionStorage) —
    // an unchecked box must mean the sign-in doesn't outlive this tab, not just that the form field
    // won't be pre-filled next time.
    if (t) setToken(t, remember);
    enter(remember);
    if (remember && t) {
      localStorage.setItem(REMEMBER_TOKEN_KEY, t);
      localStorage.setItem(REMEMBER_FLAG_KEY, "true");
    } else {
      localStorage.removeItem(REMEMBER_TOKEN_KEY);
      localStorage.removeItem(REMEMBER_FLAG_KEY);
    }
  };

  const heroLogo = (
    <div className="w-14 h-14 rounded-2xl grid place-items-center bg-gradient-to-br from-sky-400 to-blue-600 border border-white/20 shadow-lg">
      <Hexagon size={28} className="text-white" fill="currentColor" />
    </div>
  );

  return (
    <div className="min-h-screen min-h-[100dvh] flex flex-col">
      <PremiumLoginShell
        variant="secure"
        accent="orange"
        pageThemeClass="atlas-login"
        themeSwitcher={<LoginThemeSwitcher />}
        logo={heroLogo}
        productName={PRODUCT}
        productSubtitle="Storage control plane · Ceph & beyond"
        heroHeadline={
          <>
            <span className="login-text-gradient">Command your storage</span>
            <br />
            with Zeus-grade clarity
          </>
        }
        heroSubheadline="Volumes, snapshots, DataBridge, and day-2 ops — unified in a cockpit built for operators who need answers, not dashboards."
        features={FEATURES}
        pills={[
          { icon: <Sparkles className="w-3 h-3" />, label: "Observatory", glow: true },
          { icon: <Shield className="w-3 h-3" />, label: "Token RBAC" },
          { label: "Ceph · NFS · ZFS" },
        ]}
        mobileSubtitle="Zyvor Storage Control Plane"
        panelTitle="Welcome back"
        panelSubtitle={
          hostLabel ? (
            <>
              Sign in on <span className="font-mono text-foreground/80">{hostLabel}</span>
            </>
          ) : (
            "Sign in to your storage console"
          )
        }
        panelHint={
          <>
            Service-account token is optional when auth is not enforced.
            {hostLabel ? (
              <span className="block mt-1">
                Gateway on <span className="font-mono">{hostLabel}</span>
                {ver ? ` · v${ver}` : ""}
              </span>
            ) : null}
          </>
        }
        footer={<ZyvorFooter />}
      >
        <form onSubmit={go} autoComplete="on" className="text-left">
          <div className="mb-5">
            <span className="inline-flex items-center gap-1.5 rounded-lg border border-sky-400/30 bg-sky-500/10 px-2.5 py-1 text-[11px] font-medium text-sky-300/90">
              <Shield className="h-3 w-3" aria-hidden />
              Service-account token
            </span>
          </div>

          <div className="space-y-5">
            <LoginField label="Bearer token" id="atlas-token">
              <KeyRound className="login-field-icon" />
              <input
                id="atlas-token"
                name="token"
                type="password"
                value={token}
                onChange={(e) => setDraft(e.target.value)}
                autoComplete="off"
                autoFocus
                placeholder="eyJhbGciOi… (optional)"
                className="login-input"
              />
            </LoginField>
          </div>

          <LoginRemember
            checked={remember}
            onChange={setRemember}
            label="Remember token on this device"
            hint="Stored in localStorage — only for lab / trusted workstations."
          />

          <LoginSubmit loading={false} disabled={false} className="mt-7">
            <span className="relative z-10">Sign in to {PRODUCT}</span>
            <ArrowRight className="h-4 w-4 relative z-10 group-hover:translate-x-0.5 transition-transform" />
          </LoginSubmit>

          <div className="mt-5 pt-4 border-t border-white/10 flex items-center justify-between gap-2 text-xs text-muted-foreground">
            <span className="flex items-center gap-1.5">
              <span
                className={`w-1.5 h-1.5 rounded-full ${
                  health === "ok" ? "bg-success" : health === "down" ? "bg-danger" : "bg-muted"
                }`}
              />
              gateway {health}
              {ver ? ` · v${ver}` : ""}
            </span>
            <span className="flex items-center gap-1.5">
              <CheckCircle className="h-3.5 w-3.5 text-success/70 shrink-0" aria-hidden />
              Token session
            </span>
          </div>
        </form>
      </PremiumLoginShell>
    </div>
  );
}
