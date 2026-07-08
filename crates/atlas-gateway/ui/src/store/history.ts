// Copyright (c) 2026 ZyvorAI Labs Private Limited. All rights reserved.
// Client-side rolling history of /metrics/summary so the Command Deck can draw live trends
// (the gateway keeps only point-in-time snapshots; real time-series lives in Prometheus).
import type { MetricsSummary } from "../api/types";

export interface Sample {
  t: number;
  usedPct: number;
  usedBytes: number;
  readIops: number;
  writeIops: number;
}

const MAX = 60;
let buf: Sample[] = [];
let last: { t: number; r: number; w: number } | null = null;
const subs = new Set<() => void>();

export function recordSummary(s: MetricsSummary) {
  const now = Date.now();
  const r = s.client_io?.read_ops_total || 0;
  const w = s.client_io?.write_ops_total || 0;
  let readIops = 0;
  let writeIops = 0;
  if (last) {
    const dt = Math.max(1, (now - last.t) / 1000);
    readIops = Math.max(0, (r - last.r) / dt);
    writeIops = Math.max(0, (w - last.w) / dt);
  }
  last = { t: now, r, w };
  const sample: Sample = { t: now, usedPct: s.used_capacity_percent || 0, usedBytes: s.used_capacity_bytes || 0, readIops, writeIops };
  // De-dupe rapid identical samples (react-query may re-emit).
  if (buf.length && now - buf[buf.length - 1].t < 800) buf[buf.length - 1] = sample;
  else buf = [...buf, sample].slice(-MAX);
  subs.forEach((f) => f());
}

export function history(): Sample[] {
  return buf;
}
export function onHistory(fn: () => void): () => void {
  subs.add(fn);
  return () => {
    subs.delete(fn);
  };
}
