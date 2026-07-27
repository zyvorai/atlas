// Copyright (c) 2026 ZyvorAI Labs Private Limited. All rights reserved.
// Proprietary software — see LICENSE in the repository root.
// https://zyvor.dev · info@zyvor.dev

/**
 * Premium login shell — split hero + form panel (Zeus / PacketWolf suite).
 * `variant="secure"`: Secure Dark Professional — no aurora/blur effects.
 */
import type { CSSProperties, ReactNode } from "react";
import { AlertCircle, Sparkles } from "lucide-react";

export type LoginOrb = {
  size: number;
  top: string;
  left: string;
  delay: string;
  duration: string;
  hue?: "blue" | "violet" | "cyan" | "red";
};

export type PremiumLoginFeature = {
  icon: ReactNode;
  title: string;
  description: string;
  gradient?: string;
  glow?: string;
  highlight?: boolean;
};

export type PremiumLoginPill = {
  icon?: ReactNode;
  label: string;
  glow?: boolean;
};

export type LoginAccent =
  | "blue"
  | "amber"
  | "orange"
  | "violet"
  | "rose"
  | "cyan"
  | "copper"
  | "steel"
  | "zeus";

const DEFAULT_ORBS: LoginOrb[] = [
  { size: 340, top: "4%", left: "6%", delay: "0s", duration: "11s", hue: "blue" },
  { size: 220, top: "55%", left: "12%", delay: "2.2s", duration: "13s", hue: "violet" },
  { size: 180, top: "18%", left: "58%", delay: "0.8s", duration: "9s", hue: "cyan" },
  { size: 400, top: "58%", left: "68%", delay: "3.2s", duration: "15s", hue: "red" },
];

const PARTICLE_SEEDS = Array.from({ length: 28 }, (_, i) => ({
  id: i,
  left: `${(i * 17 + 7) % 100}%`,
  top: `${(i * 23 + 11) % 100}%`,
  delay: `${(i % 7) * 0.45}s`,
  size: 2 + (i % 3),
}));

export type LoginShellVariant = "premium" | "secure";

export type PremiumLoginShellProps = {
  variant?: LoginShellVariant;
  accent?: LoginAccent;
  pageThemeClass?: string;
  heroWidth?: "55" | "58";
  themeSwitcher?: ReactNode;
  logo: ReactNode;
  productName: string;
  productSubtitle?: string;
  heroHeadline: ReactNode;
  heroSubheadline: string;
  pills?: PremiumLoginPill[];
  features?: PremiumLoginFeature[];
  heroFooter?: ReactNode;
  orbs?: LoginOrb[];
  mobileSubtitle?: string;
  panelTitle?: ReactNode;
  panelSubtitle?: ReactNode;
  panelHint?: ReactNode;
  footer?: ReactNode;
  formClassName?: string;
  /** `flush` = no card/box around the form — open webpage feel. */
  formSurface?: "card" | "flush";
  children: ReactNode;
};

export function PremiumLoginShell({
  variant = "premium",
  accent = "blue",
  pageThemeClass = "",
  heroWidth = "58",
  themeSwitcher,
  logo,
  productName,
  productSubtitle,
  heroHeadline,
  heroSubheadline,
  pills = [],
  features = [],
  heroFooter,
  orbs = DEFAULT_ORBS,
  mobileSubtitle,
  panelTitle = "Welcome back",
  panelSubtitle = "Sign in to continue",
  panelHint,
  footer,
  formClassName = "",
  formSurface = "card",
  children,
}: PremiumLoginShellProps) {
  const isSecure = variant === "secure";
  const accentClass = accent === "blue" ? "" : `login-accent-${accent}`;
  const heroClass = heroWidth === "55" ? "lg:w-[55%]" : "lg:w-[58%]";
  const beamClass = heroWidth === "55" ? "login-beam-w55" : "login-beam-w58";
  const pageClass = [
    "login-page",
    "flex-1 flex flex-col lg:flex-row relative overflow-hidden",
    accentClass,
    isSecure ? "login-page-secure" : "",
    formSurface === "flush" ? "login-page-flush" : "",
    pageThemeClass,
  ]
    .filter(Boolean)
    .join(" ");
  const cardClass =
    formSurface === "flush"
      ? "login-flush-form"
      : isSecure
        ? "login-secure-card p-8"
        : "login-glass login-glass-border rounded-2xl p-8 shadow-2xl";

  const featureCards = features.map((f, i) => (
    <div
      key={f.title}
      className={`login-feature-card login-fade-in flex items-start gap-4 p-4 rounded-xl ${
        f.highlight ? "login-feature-card-highlight" : isSecure ? "" : "bg-white/[0.04]"
      }`}
      style={{ animationDelay: `${0.35 + i * 0.07}s`, opacity: isSecure ? 1 : 0 }}
    >
      <div
        className={`w-10 h-10 rounded-lg flex items-center justify-center shrink-0 bg-gradient-to-br ${
          f.gradient ?? "from-sky-500/95 to-blue-600/95"
        } shadow-lg ${f.glow ?? "shadow-sky-500/25"}`}
      >
        {f.icon}
      </div>
      <div className="min-w-0">
        <div className="text-sm font-semibold text-white flex items-center gap-2">
          {f.title}
          {f.highlight ? <Sparkles className="w-3.5 h-3.5 text-warning/90 shrink-0" aria-hidden /> : null}
        </div>
        <p className="text-xs mt-1 text-muted-foreground leading-relaxed">{f.description}</p>
      </div>
    </div>
  ));

  return (
    <div className="min-h-screen flex flex-col">
      <div className={pageClass}>
        {!isSecure ? (
          <>
            <div className="login-aurora" aria-hidden />
            <div className="login-scanline" aria-hidden />
          </>
        ) : null}

        {themeSwitcher}

        <aside
          className={`login-hero hidden lg:flex ${heroClass} flex-col justify-between p-10 xl:p-12 overflow-hidden relative`}
        >
          <div className="login-hero-mesh" aria-hidden />
          {!isSecure ? <div className="login-spotlight" aria-hidden /> : null}

          {!isSecure
            ? orbs.map((orb, i) => (
                <div
                  key={i}
                  className={`login-orb login-orb-${orb.hue ?? "blue"}`}
                  style={
                    {
                      width: orb.size,
                      height: orb.size,
                      top: orb.top,
                      left: orb.left,
                      "--login-delay": orb.delay,
                      "--login-duration": orb.duration,
                    } as CSSProperties
                  }
                />
              ))
            : null}

          {!isSecure ? (
            <div className="login-particles" aria-hidden>
              {PARTICLE_SEEDS.map((p) => (
                <span
                  key={p.id}
                  className="login-particle"
                  style={{
                    left: p.left,
                    top: p.top,
                    width: p.size,
                    height: p.size,
                    animationDelay: p.delay,
                  }}
                />
              ))}
            </div>
          ) : null}

          <div className="relative z-10">
            <div className="login-fade-in flex items-center gap-4 mb-8">
              <div className="login-logo-ring">{logo}</div>
              <div>
                <span className="text-4xl font-bold tracking-tight text-white block">{productName}</span>
                {productSubtitle ? (
                  <span
                    className={`text-xs font-medium uppercase tracking-[0.28em] mt-0.5 block ${
                      isSecure ? "text-muted-foreground" : "text-sky-400/80"
                    }`}
                  >
                    {productSubtitle}
                  </span>
                ) : null}
              </div>
            </div>
            <h2 className="login-fade-in login-fade-in-d1 text-4xl xl:text-[2.75rem] font-extrabold text-white leading-[1.08] mb-4 max-w-xl">
              {heroHeadline}
            </h2>
            <p className="login-fade-in login-fade-in-d2 text-lg text-foreground/70 max-w-lg leading-relaxed">
              {heroSubheadline}
            </p>
            {pills.length > 0 ? (
              <div className="login-fade-in login-fade-in-d3 flex flex-wrap gap-2 mt-6">
                {pills.map((pill) => (
                  <span
                    key={pill.label}
                    className={`login-stat-pill${pill.glow ? " login-stat-pill-glow" : ""}`}
                  >
                    {pill.icon}
                    {pill.label}
                  </span>
                ))}
              </div>
            ) : null}
          </div>

          {features.length > 0 ? (
            <div className="relative z-10 space-y-2.5 max-h-[42vh] overflow-y-auto login-feature-scroll pr-1">
              {featureCards}
            </div>
          ) : null}

          {heroFooter ? <div className="relative z-10 login-fade-in login-fade-in-d4">{heroFooter}</div> : null}
        </aside>

        {!isSecure ? <div className={`login-beam hidden lg:block ${beamClass}`} aria-hidden /> : null}

        <main
          className="login-panel flex-1 flex items-center justify-center relative px-6 py-12 min-h-screen lg:min-h-0"
          aria-label="Sign in"
        >
          <div className="login-panel-grid" aria-hidden />
          {!isSecure ? <div className="login-panel-glow" aria-hidden /> : null}
          <div className="w-full max-w-[420px] min-w-0 relative z-10">
            <div className="lg:hidden text-center mb-8">
              <div className="login-logo-ring inline-block mb-4">{logo}</div>
              <h1 className="text-2xl font-bold text-white">{productName}</h1>
              <p className="text-sm mt-1 text-muted-foreground">
                {mobileSubtitle ?? productSubtitle ?? panelSubtitle}
              </p>
            </div>

            <div className="hidden lg:block mb-8">
              <h2 className="text-2xl font-bold mb-1 text-white">{panelTitle}</h2>
              <p className="text-sm text-muted-foreground">{panelSubtitle}</p>
            </div>

            <div className={`${cardClass} ${formClassName}`.trim()}>{children}</div>

            {panelHint ? (
              <p
                className={`text-center mt-4 max-w-sm mx-auto leading-relaxed ${
                  isSecure ? "login-trust-line" : "text-xs text-muted-foreground"
                }`}
              >
                {panelHint}
              </p>
            ) : null}
          </div>
        </main>
      </div>
      {footer}
    </div>
  );
}

export type LoginErrorVariant = "credentials" | "network" | "generic";

export function LoginError({
  message,
  variant = "generic",
}: {
  message: string;
  variant?: LoginErrorVariant;
}) {
  const title =
    variant === "network"
      ? "Connection problem"
      : variant === "credentials"
        ? "Sign-in failed"
        : "Unable to sign in";

  return (
    <div
      className="flex items-start gap-2.5 bg-destructive/50 border border-destructive/40 rounded-xl p-3 mb-6 login-shake"
      role="alert"
      aria-live="assertive"
    >
      <AlertCircle className="h-4 w-4 text-destructive shrink-0 mt-0.5" aria-hidden />
      <div>
        <p className="text-sm font-medium text-destructive">{title}</p>
        <p className="text-sm text-destructive/90 mt-0.5">{message}</p>
      </div>
    </div>
  );
}

export function LoginField({
  label,
  id,
  children,
}: {
  label: string;
  id: string;
  children: ReactNode;
}) {
  return (
    <div>
      <label htmlFor={id} className="block text-sm font-medium text-foreground/70 mb-2">
        {label}
      </label>
      <div className="relative group">{children}</div>
    </div>
  );
}

export function LoginSubmit({
  loading,
  disabled,
  children,
  className = "",
}: {
  loading?: boolean;
  disabled?: boolean;
  children: ReactNode;
  className?: string;
}) {
  return (
    <button
      type="submit"
      disabled={disabled || loading}
      className={`login-btn-primary group ${className}`.trim()}
    >
      {children}
    </button>
  );
}

export function LoginRemember({
  checked,
  onChange,
  label = "Remember me on this device",
  hint,
}: {
  checked: boolean;
  onChange: (checked: boolean) => void;
  label?: string;
  hint?: string;
}) {
  return (
    <div className="mt-5">
      <label className="flex items-center gap-2.5 cursor-pointer select-none">
        <input
          type="checkbox"
          checked={checked}
          onChange={(e) => onChange(e.target.checked)}
          className="w-4 h-4 rounded border-border bg-card accent-primary"
        />
        <span className="text-sm text-muted-foreground">{label}</span>
      </label>
      {hint ? <p className="text-xs text-muted-foreground mt-1.5 ml-[1.625rem]">{hint}</p> : null}
    </div>
  );
}

export function LoginDivider({ label = "or" }: { label?: string }) {
  return (
    <div className="relative py-3 mt-4 text-center text-xs uppercase tracking-[0.22em] text-muted-foreground">
      <span className="relative px-2 bg-card/40">{label}</span>
      <div className="absolute inset-x-0 top-1/2 -translate-y-1/2 border-t border-border/60" />
    </div>
  );
}

/** @deprecated typo guard — use PremiumLoginShell */
export const PremumLoginShell = PremiumLoginShell;
