// Copyright (c) 2026 ZyvorAI Labs Private Limited. All rights reserved.
// Bespoke primitive kit styled with the vendored Zeus/Tahoe design foundation.
import React, { useEffect, useState } from "react";
import { createPortal } from "react-dom";
import { Loader2, X } from "lucide-react";
import { cx } from "../lib/format";

/** Close an overlay on Escape. */
function useEscape(active: boolean, onClose: () => void) {
  useEffect(() => {
    if (!active) return;
    const h = (e: KeyboardEvent) => e.key === "Escape" && onClose();
    window.addEventListener("keydown", h);
    return () => window.removeEventListener("keydown", h);
  }, [active, onClose]);
}

type Variant = "primary" | "secondary" | "ghost" | "danger";
const VAR: Record<Variant, string> = {
  primary: "btn-primary",
  secondary: "btn-secondary",
  ghost: "btn-ghost",
  danger: "btn-danger",
};

export function Button({
  variant = "secondary",
  size,
  loading,
  icon: Icon,
  children,
  className,
  ...rest
}: {
  variant?: Variant;
  size?: "sm";
  loading?: boolean;
  icon?: React.ComponentType<{ size?: number }>;
  children?: React.ReactNode;
} & React.ButtonHTMLAttributes<HTMLButtonElement>) {
  return (
    <button
      {...rest}
      disabled={rest.disabled || loading}
      className={cx("btn", VAR[variant], size === "sm" && "btn-sm", className)}
    >
      {loading ? <Loader2 size={14} className="animate-spin" /> : Icon ? <Icon size={14} /> : null}
      {children}
    </button>
  );
}

export function Card({ className, children, hoverable }: { className?: string; children: React.ReactNode; hoverable?: boolean }) {
  return <div className={cx("glass-card", hoverable && "hoverable", className)}>{children}</div>;
}

export function GlassSection({
  title,
  actions,
  children,
  className,
}: {
  title?: React.ReactNode;
  actions?: React.ReactNode;
  children: React.ReactNode;
  className?: string;
}) {
  return (
    <section className={cx("glass-card overflow-hidden", className)}>
      {(title || actions) && (
        <div className="flex items-center gap-3 px-4 py-3 border-b border-white/[0.06]">
          <div className="text-[13px] font-semibold flex-1">{title}</div>
          {actions}
        </div>
      )}
      <div>{children}</div>
    </section>
  );
}

export function StatCard({
  label,
  value,
  sub,
  accent,
  children,
}: {
  label: string;
  value: React.ReactNode;
  sub?: React.ReactNode;
  accent?: string;
  children?: React.ReactNode;
}) {
  return (
    <Card hoverable className="p-4">
      <div className="section-label">{label}</div>
      <div className="text-2xl font-bold mt-1.5" style={{ color: accent }}>
        {value}
      </div>
      {sub && <div className="text-xs text-muted-foreground mt-1">{sub}</div>}
      {children}
    </Card>
  );
}

type BadgeKind = "success" | "warning" | "danger" | "info" | "neutral";
export function Badge({ kind = "neutral", dot, title, children }: { kind?: BadgeKind; dot?: boolean; title?: string; children: React.ReactNode }) {
  return (
    <span className={cx("badge", `badge-${kind}`)} title={title}>
      {dot && <span className="dot" />}
      {children}
    </span>
  );
}

export function Field(props: React.InputHTMLAttributes<HTMLInputElement>) {
  return <input {...props} className={cx("field", props.className)} />;
}
export function Select(props: React.SelectHTMLAttributes<HTMLSelectElement>) {
  return <select {...props} className={cx("field", props.className)} />;
}
export function Label({ children }: { children: React.ReactNode }) {
  return <div className="section-label mb-1 mt-3">{children}</div>;
}

export function PageHeader({
  icon: Icon,
  title,
  subtitle,
  actions,
}: {
  icon?: React.ComponentType<{ size?: number }>;
  title: string;
  subtitle?: string;
  actions?: React.ReactNode;
}) {
  return (
    <div className="flex items-center gap-3 mb-5">
      {Icon && (
        <div className="w-10 h-10 rounded-xl glass grid place-items-center text-sky-400">
          <Icon size={20} />
        </div>
      )}
      <div className="flex-1">
        <h1 className="text-xl font-bold tracking-tight">{title}</h1>
        {subtitle && <div className="text-sm text-muted-foreground">{subtitle}</div>}
      </div>
      <div className="flex items-center gap-2">{actions}</div>
    </div>
  );
}

export function EmptyState({ msg, cta }: { msg: string; cta?: React.ReactNode }) {
  return (
    <div className="py-10 text-center text-sm text-muted-foreground">
      <div>{msg}</div>
      {cta && <div className="mt-3 flex justify-center">{cta}</div>}
    </div>
  );
}

/** Monospace text that copies to the clipboard on click. */
export function Copyable({ text, className }: { text: string; className?: string }) {
  const [ok, setOk] = useState(false);
  return (
    <button
      className={cx("mono hover:text-sky-300 transition text-left", className)}
      title="Click to copy"
      onClick={(e) => {
        e.stopPropagation();
        navigator.clipboard
          ?.writeText(text)
          .then(() => {
            setOk(true);
            setTimeout(() => setOk(false), 1000);
          })
          .catch(() => {});
      }}
    >
      {ok ? "copied ✓" : text}
    </button>
  );
}

/** SVG radial capacity gauge, colored by threshold. */
export function RadialGauge({ pct, size = 120, label }: { pct: number; size?: number; label?: string }) {
  const r = size / 2 - 10;
  const c = 2 * Math.PI * r;
  const p = Math.max(0, Math.min(100, pct));
  const color = p >= 90 ? "#E23B3B" : p >= 75 ? "#FBBF24" : "#38BDF8";
  return (
    <div className="relative grid place-items-center" style={{ width: size, height: size }}>
      <svg width={size} height={size} className="-rotate-90">
        <circle cx={size / 2} cy={size / 2} r={r} fill="none" stroke="rgba(255,255,255,0.06)" strokeWidth={9} />
        <circle
          cx={size / 2} cy={size / 2} r={r} fill="none" stroke={color} strokeWidth={9} strokeLinecap="round"
          strokeDasharray={c} strokeDashoffset={c - (p / 100) * c} style={{ transition: "stroke-dashoffset .6s ease" }}
        />
      </svg>
      <div className="absolute text-center">
        <div className="text-2xl font-bold" style={{ color }}>{p.toFixed(0)}%</div>
        {label && <div className="text-[11px] text-muted-foreground">{label}</div>}
      </div>
    </div>
  );
}
export function Spinner() {
  return (
    <div className="py-10 grid place-items-center text-muted-foreground">
      <Loader2 className="animate-spin" />
    </div>
  );
}

export function Tabs({ tabs, value, onChange }: { tabs: string[]; value: string; onChange: (t: string) => void }) {
  return (
    <div className="flex gap-1 p-1 rounded-full glass w-fit mb-4">
      {tabs.map((t) => (
        <button
          key={t}
          onClick={() => onChange(t)}
          className={cx(
            "px-3.5 py-1.5 rounded-full text-xs font-semibold transition",
            value === t ? "bg-white/10 text-white" : "text-muted-foreground hover:text-white",
          )}
        >
          {t}
        </button>
      ))}
    </div>
  );
}

export function SlideOver({
  open,
  onClose,
  title,
  children,
  width = 460,
}: {
  open: boolean;
  onClose: () => void;
  title: React.ReactNode;
  children: React.ReactNode;
  width?: number;
}) {
  useEscape(open, onClose);
  if (!open) return null;
  return createPortal(
    <div className="fixed inset-0 z-50" onMouseDown={onClose}>
      <div className="absolute inset-0 bg-black/50 backdrop-blur-sm" />
      <div
        className="absolute right-0 top-0 h-full glass-card rounded-none border-l overflow-auto animate-fade-in"
        style={{ width, maxWidth: "92vw" }}
        onMouseDown={(e) => e.stopPropagation()}
      >
        <div className="flex items-center gap-3 px-4 py-3 border-b border-white/[0.06] sticky top-0 bg-card/80 backdrop-blur">
          <div className="font-semibold flex-1">{title}</div>
          <button className="btn btn-ghost btn-sm" onClick={onClose}>
            <X size={14} />
          </button>
        </div>
        <div className="p-4">{children}</div>
      </div>
    </div>,
    document.body,
  );
}

export function Modal({
  open,
  onClose,
  title,
  children,
  footer,
}: {
  open: boolean;
  onClose: () => void;
  title: React.ReactNode;
  children: React.ReactNode;
  footer?: React.ReactNode;
}) {
  useEscape(open, onClose);
  if (!open) return null;
  return createPortal(
    <div className="fixed inset-0 z-50 grid place-items-center p-4" onMouseDown={onClose}>
      <div className="absolute inset-0 bg-black/60 backdrop-blur-sm" />
      <div
        className="relative glass-card w-[460px] max-w-[94vw] p-5 animate-fade-in"
        onMouseDown={(e) => e.stopPropagation()}
      >
        <div className="flex items-center gap-3 mb-3">
          <div className="font-semibold flex-1">{title}</div>
          <button className="btn btn-ghost btn-sm" onClick={onClose}>
            <X size={14} />
          </button>
        </div>
        {children}
        {footer && <div className="flex justify-end gap-2 mt-4">{footer}</div>}
      </div>
    </div>,
    document.body,
  );
}

/** A tiny declarative form modal — fields produce a values object on submit. */
export type FormField = {
  name: string;
  label: string;
  type?: "text" | "number";
  value?: string;
  placeholder?: string;
  options?: { value: string; label: string }[];
  optional?: boolean;
};
export function FormModal({
  open,
  onClose,
  title,
  fields,
  submitLabel = "Create",
  onSubmit,
}: {
  open: boolean;
  onClose: () => void;
  title: React.ReactNode;
  fields: FormField[];
  submitLabel?: string;
  onSubmit: (v: Record<string, string>) => Promise<void> | void;
}) {
  const [vals, setVals] = useState<Record<string, string>>({});
  const [busy, setBusy] = useState(false);
  useEffect(() => {
    if (open) {
      const init: Record<string, string> = {};
      fields.forEach((f) => (init[f.name] = f.value ?? (f.options ? f.options[0]?.value ?? "" : "")));
      setVals(init);
    }
  }, [open]);
  const set = (k: string, v: string) => setVals((s) => ({ ...s, [k]: v }));
  // Select fields always carry a value (they default to the first option); only plain text/number
  // fields can be left blank, so only those need a required check.
  const missingRequired = fields.some((f) => !f.optional && !f.options && !(vals[f.name] ?? "").trim());
  return (
    <Modal
      open={open}
      onClose={onClose}
      title={title}
      footer={
        <>
          <Button onClick={onClose}>Cancel</Button>
          <Button
            variant="primary"
            loading={busy}
            disabled={missingRequired}
            onClick={async () => {
              setBusy(true);
              try {
                await onSubmit(vals);
                onClose();
              } catch {
                // onSubmit is submit()/submitJob(), which already toasts the error and rethrows so
                // callers can keep the modal open on failure. Swallow it here — otherwise, with no
                // catch, it becomes an unhandled promise rejection on every failed create/write.
              } finally {
                setBusy(false);
              }
            }}
          >
            {submitLabel}
          </Button>
        </>
      }
    >
      {fields.map((f) => (
        <div key={f.name}>
          <Label>{f.label}</Label>
          {f.options ? (
            <Select value={vals[f.name] ?? ""} onChange={(e) => set(f.name, e.target.value)}>
              {f.options.map((o) => (
                <option key={o.value} value={o.value}>
                  {o.label}
                </option>
              ))}
            </Select>
          ) : (
            <Field
              type={f.type || "text"}
              placeholder={f.placeholder}
              value={vals[f.name] ?? ""}
              onChange={(e) => set(f.name, e.target.value)}
            />
          )}
        </div>
      ))}
    </Modal>
  );
}
