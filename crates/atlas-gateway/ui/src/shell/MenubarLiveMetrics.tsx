// Copyright (c) 2026 ZyvorAI Labs Private Limited. All rights reserved.
/** Capacity sparkline + cluster pulse — Zeus MenubarLiveMetrics, Atlas metrics. */
import { useEffect, useRef } from "react";
import { Link } from "react-router-dom";
import { Activity } from "lucide-react";
import { useClusters, useSummary } from "../api/hooks";

export function MenubarLiveMetrics() {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const historyRef = useRef<number[]>([]);
  const { data: summary } = useSummary();
  const { data: clusters, isError } = useClusters();

  const pct = summary?.used_capacity_percent;
  const hasPct = pct != null && Number.isFinite(pct);
  const health = isError ? "auth" : clusters?.[0]?.health || "unknown";
  const healthClass =
    health === "ok" ? "ok" : health === "warn" ? "warn" : health === "critical" ? "crit" : "muted";

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
    const stroke = rootStyle.getPropertyValue("--at-cyan").trim() || "#3fd0e8";
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
    summary ? `${summary.volumes} volumes · ${summary.pools} pools` : null,
    isError ? "Auth required" : `Cluster ${String(health).toUpperCase()}`,
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
      <span className="at-menubar-metrics-pct">{hasPct ? `${Math.round(pct)}%` : "—"}</span>
      <span className={`at-menubar-pulse ${healthClass}`} aria-hidden />
    </Link>
  );
}
