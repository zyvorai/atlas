// Copyright (c) 2026 ZyvorAI Labs Private Limited. All rights reserved.
import { clsx, type ClassValue } from "clsx";
import { twMerge } from "tailwind-merge";

export const cx = (...a: ClassValue[]) => twMerge(clsx(a));

export function fmtBytes(n?: number | null): string {
  const v = Number(n) || 0;
  const u = ["B", "KiB", "MiB", "GiB", "TiB", "PiB"];
  let i = 0;
  let x = v;
  while (x >= 1024 && i < u.length - 1) {
    x /= 1024;
    i++;
  }
  return `${i ? x.toFixed(1) : x} ${u[i]}`;
}

export const num = (n?: number | null) => (Number(n) || 0).toLocaleString();

export function timeAgo(iso?: string | null): string {
  if (!iso) return "—";
  const d = new Date(iso).getTime();
  if (Number.isNaN(d)) return iso;
  const s = Math.floor((Date.now() - d) / 1000);
  // A future timestamp (e.g. a schedule's next_run_at) makes `s` negative — "-3595s ago" instead
  // of "in 59m" otherwise, since every unit branch below treated any s < 60 as "just now, past".
  if (s < 0) {
    const f = -s;
    if (f < 60) return `in ${f}s`;
    if (f < 3600) return `in ${Math.floor(f / 60)}m`;
    if (f < 86400) return `in ${Math.floor(f / 3600)}h`;
    return `in ${Math.floor(f / 86400)}d`;
  }
  if (s < 60) return `${s}s ago`;
  if (s < 3600) return `${Math.floor(s / 60)}m ago`;
  if (s < 86400) return `${Math.floor(s / 3600)}h ago`;
  return `${Math.floor(s / 86400)}d ago`;
}

export const gib = (g: number) => g * 1024 * 1024 * 1024;

export function healthKind(h?: string): "success" | "warning" | "danger" | "neutral" {
  if (h === "ok") return "success";
  if (h === "warn" || h === "warning") return "warning";
  if (h === "critical" || h === "unauthenticated") return "danger";
  return "neutral";
}

export function stateKind(s?: string): "success" | "warning" | "danger" | "info" | "neutral" {
  const ok = ["succeeded", "verified", "bound", "available", "ok", "completed", "ready"];
  const bad = ["failed", "critical", "error", "lost"];
  const run = ["running", "queued", "pending", "verifying", "creating", "progressing"];
  if (ok.includes(s || "")) return "success";
  if (bad.includes(s || "")) return "danger";
  if (run.includes(s || "")) return "info";
  if (s === "open" || s === "warn" || s === "warning") return "warning";
  return "neutral";
}
