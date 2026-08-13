// Copyright (c) 2026 ZyvorAI Labs Private Limited. All rights reserved.
// Atlas sign-in — open webpage composition (no card / no split panel box).
import { useEffect, useState, type FormEvent } from "react";
import { ArrowRight, Hexagon, Palette } from "lucide-react";
import { useUi, type Theme } from "../store/ui";
import { API_BASE, clearToasts } from "../api/client";

const PRODUCT = "Atlas";
const REMEMBER_USER_KEY = "atlas.login-remember-user";
const REMEMBER_FLAG_KEY = "atlas.login-remember";
const DEFAULT_USER = "admin";

const THEME_OPTIONS: { id: Theme; label: string }[] = [
  { id: "carbon", label: "Carbon" },
  { id: "nebula", label: "Nebula" },
  { id: "dark", label: "Dark steel" },
  { id: "zinc", label: "Zinc metal" },
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
    <div id="atlas-login-theme" className="atlas-signin-theme">
      <button
        type="button"
        onClick={() => setOpen((v) => !v)}
        title="Change theme"
        aria-label="Change theme"
        aria-expanded={open}
        className="atlas-signin-theme-btn"
      >
        <Palette className="w-4 h-4" aria-hidden />
      </button>
      {open && (
        <div className="atlas-signin-theme-menu" role="group" aria-label="Visual theme">
          {THEME_OPTIONS.map(({ id, label }) => (
            <button
              key={id}
              type="button"
              onClick={() => {
                setTheme(id);
                setOpen(false);
              }}
              className={theme === id ? "is-active" : undefined}
            >
              {label}
            </button>
          ))}
        </div>
      )}
    </div>
  );
}

export function Login() {
  const enter = useUi((s) => s.enter);
  const setToken = useUi((s) => s.setToken);
  const sessionHint = useUi((s) => s.sessionHint);
  const clearSessionHint = useUi((s) => s.clearSessionHint);
  const [username, setUsername] = useState(DEFAULT_USER);
  const [password, setPassword] = useState("");
  const [remember, setRemember] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [ver, setVer] = useState("");
  const [health, setHealth] = useState<"ok" | "down" | "…">("…");
  const [ssoEnabled, setSsoEnabled] = useState(false);
  const hostLabel = typeof window !== "undefined" ? window.location.hostname : "";

  useEffect(() => {
    document.title = `Sign in · ${PRODUCT}`;
    clearToasts();
    // Drop legacy token-remember keys from the old bearer-only gate.
    localStorage.removeItem("atlas.login-remember-token");
    const remembered = localStorage.getItem(REMEMBER_FLAG_KEY) === "true";
    const savedUser = localStorage.getItem(REMEMBER_USER_KEY);
    if (remembered && savedUser) {
      setUsername(savedUser);
      setRemember(true);
    }
    fetch("/version")
      .then((r) => r.json())
      .then((v) => setVer(v.version))
      .catch(() => {});
    fetch("/health")
      .then((r) => setHealth(r.ok ? "ok" : "down"))
      .catch(() => setHealth("down"));
    fetch(`${API_BASE}/auth/oidc/status`)
      .then((r) => r.json())
      .then((v) => setSsoEnabled(Boolean(v?.enabled)))
      .catch(() => setSsoEnabled(false));
    return () => {
      document.title = PRODUCT;
    };
  }, []);

  const go = async (e?: FormEvent) => {
    e?.preventDefault();
    if (busy) return;
    const user = username.trim();
    clearSessionHint();
    setError(null);
    if (!user || !password) {
      setError("Enter username and password.");
      return;
    }
    setBusy(true);
    try {
      const res = await fetch(`${API_BASE}/auth/login`, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ username: user, password }),
      });
      const body = await res.json().catch(() => ({}));
      if (!res.ok) {
        setError(body?.error?.message || (res.status === 401 ? "Invalid username or password." : `Sign-in failed (${res.status}).`));
        return;
      }
      const jwt = typeof body?.token === "string" ? body.token : "";
      if (!jwt) {
        setError("Gateway returned no session token.");
        return;
      }
      setToken(jwt, remember);
      enter(remember);
      if (remember) {
        localStorage.setItem(REMEMBER_USER_KEY, user);
        localStorage.setItem(REMEMBER_FLAG_KEY, "true");
      } else {
        localStorage.removeItem(REMEMBER_USER_KEY);
        localStorage.removeItem(REMEMBER_FLAG_KEY);
      }
    } catch {
      setError("Could not reach the gateway. Check the URL and try again.");
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="atlas-signin">
      <div className="atlas-signin-atmosphere" aria-hidden>
        <div className="atlas-signin-wash" />
        <div className="atlas-signin-mesh" />
        <div className="atlas-signin-beam" />
      </div>

      <LoginThemeSwitcher />

      <header className="atlas-signin-brand">
        <Hexagon className="atlas-signin-mark" aria-hidden />
        <div>
          <p className="atlas-signin-product">{PRODUCT}</p>
          <p className="atlas-signin-kicker">Storage Center</p>
        </div>
      </header>

      <main className="atlas-signin-main">
        <section className="atlas-signin-copy">
          <h1 className="atlas-signin-title">
            Survey the cluster
            <span className="atlas-signin-title-muted"> before you steer it</span>
          </h1>
          <p className="atlas-signin-lede">
            Capacity, volumes, Ceph, and DataBridge — Atlas Storage Center for operators who chart
            the fleet, not a wall of widgets.
          </p>
        </section>

        <section className="atlas-signin-gate" aria-label="Sign in">
          <form onSubmit={go} autoComplete="on" className="atlas-signin-form">
            {(error || sessionHint) && (
              <p
                className={error ? "atlas-signin-error" : "atlas-signin-hint"}
                role={error ? "alert" : "status"}
              >
                {error || sessionHint}
              </p>
            )}

            <label className="atlas-signin-label" htmlFor="atlas-username">
              Username
            </label>
            <input
              id="atlas-username"
              name="username"
              type="text"
              value={username}
              onChange={(e) => {
                setUsername(e.target.value);
                if (error) setError(null);
              }}
              autoComplete="username"
              autoFocus
              placeholder="admin"
              className="atlas-signin-input"
              disabled={busy}
            />

            <label className="atlas-signin-label atlas-signin-label--next" htmlFor="atlas-password">
              Password
            </label>
            <input
              id="atlas-password"
              name="password"
              type="password"
              value={password}
              onChange={(e) => {
                setPassword(e.target.value);
                if (error) setError(null);
              }}
              autoComplete="current-password"
              placeholder="••••••••"
              className="atlas-signin-input"
              disabled={busy}
            />

            <label className="atlas-signin-remember">
              <input
                type="checkbox"
                checked={remember}
                onChange={(e) => setRemember(e.target.checked)}
                disabled={busy}
              />
              <span>Remember on this device</span>
            </label>

            <button type="submit" className="atlas-signin-submit" disabled={busy}>
              <span>{busy ? "Signing in…" : `Sign in to ${PRODUCT}`}</span>
              {!busy ? <ArrowRight className="w-4 h-4" aria-hidden /> : null}
            </button>

            {ssoEnabled && (
              <>
                <div className="atlas-signin-divider" role="separator">
                  <span>or</span>
                </div>
                <button
                  type="button"
                  className="atlas-signin-sso"
                  disabled={busy}
                  onClick={() => {
                    // Full-page navigation, not fetch — the identity provider needs a real
                    // browser round-trip (it may set its own cookies / can't run in an iframe).
                    window.location.href = `${API_BASE}/auth/oidc/login`;
                  }}
                >
                  Sign in with SSO
                </button>
              </>
            )}

            <p className="atlas-signin-meta">
              <span
                className={`atlas-signin-dot atlas-signin-dot--${
                  health === "ok" ? "ok" : health === "down" ? "down" : "wait"
                }`}
              />
              {hostLabel || "gateway"}
              {ver ? ` · v${ver}` : ""}
              {" · "}
              {health === "…" ? "checking" : health}
            </p>
          </form>
        </section>
      </main>

      <footer className="atlas-signin-footer" role="contentinfo">
        <a href="https://zyvor.dev" target="_blank" rel="noopener noreferrer">
          zyvor.dev
        </a>
        <span aria-hidden>·</span>
        <span>Atlas</span>
        <span aria-hidden>·</span>
        <span>© 2026</span>
      </footer>
    </div>
  );
}
