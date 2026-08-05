// Copyright (c) 2026 ZyvorAI Labs Private Limited. All rights reserved.
/** Capacity sparkline + IOPS/recovery/cluster pulse — Zeus MenubarLiveMetrics, Atlas metrics. */
import { useEffect, useMemo, useRef } from "react";
import { Link } from "react-router-dom";
import { Activity } from "lucide-react";
import { isUnauthorized } from "../api/client";
import { useClusters, useSummary } from "../api/hooks";

function fmtOps(n: number | undefined): string {
  if (n == null || !Number.isFinite(n)) return "—";
  if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(1)}M`;
  if (n >= 1_000) return `${(n / 1_000).toFixed(1)}k`;
  return String(Math.round(n));
}

export function MenubarLiveMetrics() {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const historyRef = useRef<number[]>([]);
  const { data: summary } = useSummary();
  const { data: clusters, isError, error } = useClusters();

  const pct = summary?.used_capacity_percent;
  const hasPct = pct != null && Number.isFinite(pct);
  // Prefer last-known cluster health over mislabeling network flaps as auth failures.
  const health =
    isError && isUnauthorized(error) ? "auth" : clusters?.[0]?.health || (isError ? "unknown" : "unknown");
  const healthClass =
    health === "ok"
      ? "ok"
      : health === "warn"
        ? "warn"
        : health === "critical" || health === "auth"
          ? "crit"
          : "muted";

  const readOps = summary?.client_io?.read_ops_total;
  const writeOps = summary?.client_io?.write_ops_total;
  const recovering =
    (summary?.recovery?.pg_recovering ?? 0) + (summary?.recovery?.pg_backfilling ?? 0);
  const degraded = summary?.recovery?.objects_degraded ?? 0;
  const recoveryHot = recovering > 0 || degraded > 0;

  const iopsLabel = useMemo(() => {
    if (readOps == null && writeOps == null) return null;
    return `${fmtOps(readOps)}/${fmtOps(writeOps)}`;
  }, [readOps, writeOps]);

  useEffect(() => {
    if (!hasPct) return;
    historyRef.current = [...historyRef.current.slice(-11), pct];
    const canvas = canvasRef.current;
    if (!canvas) return;
    const ctx = canvas.getContext("2d");
    if (!ctx) return;

    const dpr = window.devicePixelRatio || 1;
    const w = 36;
    const h = 14;
    canvas.width = w * dpr;
    canvas.height = h * dpr;
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    ctx.clearRect(0, 0, w, h);

    const values = historyRef.current;
    if (values.length < 2) return;
    const min = Math.min(...values);
    const max = Math.max(...values);
    const range = max - min || 1;

    const rootStyle = getComputedStyle(document.documentElement);
    const stroke = rootStyle.getPropertyValue("--d2").trim() || "#60a5fa";
    ctx.strokeStyle = stroke;
    ctx.lineWidth = 1.5;
    ctx.beginPath();
    values.forEach((v, i) => {
      const x = (i / (values.length - 1)) * w;
      const y = h - ((v - min) / range) * (h - 2) - 1;
      if (i === 0) ctx.moveTo(x, y);
      else ctx.lineTo(x, y);
    });
    ctx.stroke();
  }, [pct, hasPct]);

  const tooltip = [
    hasPct ? `Capacity ${Math.round(pct)}% used` : "Capacity unavailable",
    iopsLabel ? `Client I/O R/W ops ${iopsLabel}` : null,
    recoveryHot
      ? `Recovery · ${recovering} PG · ${degraded} degraded objs`
      : "Recovery idle",
    summary ? `${summary.volumes} volumes · ${summary.pools} pools` : null,
    isError && isUnauthorized(error)
      ? "Auth required"
      : isError
        ? "Cluster status unavailable"
        : `Cluster ${String(health).toUpperCase()}`,
  ]
    .filter(Boolean)
    .join(" · ");

  return (
    <Link
      to="/observatory"
      className={`at-menubar-metrics ${healthClass}`}
      title={tooltip}
      aria-label={tooltip}
    >
      <Activity size={14} strokeWidth={2} aria-hidden />
      <canvas ref={canvasRef} className={hasPct ? undefined : "dim"} aria-hidden />
      <span className="at-menubar-chip" data-kind="cap">
        {hasPct ? `${Math.round(pct)}%` : "—"}
      </span>
      {iopsLabel && (
        <span className="at-menubar-chip" data-kind="io" title="Client read/write ops (lifetime counters)">
          I/O {iopsLabel}
        </span>
      )}
      <span
        className={`at-menubar-chip ${recoveryHot ? "hot" : ""}`}
        data-kind="rec"
        title={
          recoveryHot
            ? `${recovering} recovering/backfilling PGs · ${degraded} degraded objects`
            : "No recovery activity"
        }
      >
        {recoveryHot ? `REC ${recovering || degraded}` : "REC 0"}
      </span>
      <span className={`at-menubar-pulse ${healthClass}`} aria-hidden />
    </Link>
  );
}
