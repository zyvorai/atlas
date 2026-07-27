// Copyright (c) 2026 ZyvorAI Labs Private Limited. All rights reserved.
import { useState } from "react";
import { useJobs } from "../api/hooks";
import { useUi } from "../store/ui";
import { Badge, Copyable, SlideOver } from "../ui/kit";
import { PageHead } from "../ui/PageHead";
import { Table } from "../ui/Table";
import { stateKind, timeAgo } from "../lib/format";

function Bar({ pct, kind }: { pct: number; kind: string }) {
  const c = kind === "failed" ? "#E23B3B" : kind === "succeeded" ? "#30D69E" : "#38BDF8";
  return (
    <div className="w-28 h-1.5 rounded-full bg-white/5 overflow-hidden">
      <div className="h-full rounded-full" style={{ width: `${Math.min(100, pct || 0)}%`, background: c }} />
    </div>
  );
}

export default function Jobs() {
  const { data } = useJobs();
  const live = Object.values(useUi((s) => s.jobs));
  const [selId, setSelId] = useState<string | null>(null);
  const sel = (data || []).find((j) => j.id === selId) || null;
  const n = data?.length || 0;
  const running = (data || []).filter((j) => j.state === "running" || j.state === "queued").length;
  return (
    <div>
      <PageHead
        eyebrow="STORAGE · INDEX"
        title="Jobs"
        state={
          live.length
            ? `${live.length} live this session · ${n} recent — async ops via server-sent events.`
            : running
              ? `${running} in flight · ${n} recent on the job engine.`
              : n
                ? `${n} recent job${n === 1 ? "" : "s"} — click a row for result detail.`
                : "No jobs yet. Provisioning and day-2 ops appear here as they run."
        }
      />
      {live.length > 0 && (
        <div style={{ marginBottom: 16 }}>
          <Table
            soundings
            panelTitle="Live · this session"
            rows={live}
            rowKey={(j) => j.id}
            cols={[
              { h: "Job", f: (j) => j.label },
              { h: "State", f: (j) => <Badge kind={stateKind(j.state)} dot>{j.state}</Badge> },
              { h: "Progress", f: (j) => <Bar pct={j.progress} kind={j.state} /> },
              { h: "ID", f: (j) => j.id, mono: true },
            ]}
          />
        </div>
      )}
      <Table
        soundings
        panelTitle="Recent jobs"
        rows={data}
        rowKey={(j) => j.id}
        onRow={(j) => setSelId(j.id)}
        empty="No jobs recorded yet."
        cols={[
          { h: "ID", f: (j) => <Copyable text={j.id} />, mono: true },
          { h: "Type", f: (j) => j.job_type, mono: true, sortKey: (j) => j.job_type },
          { h: "State", f: (j) => <Badge kind={stateKind(j.state)} dot>{j.state}</Badge>, sortKey: (j) => j.state },
          { h: "Progress", f: (j) => <Bar pct={j.progress_percent} kind={j.state} /> },
          { h: "Error", f: (j) => <span className="text-danger">{j.error || ""}</span> },
          { h: "When", f: (j) => <span className="text-muted-foreground">{timeAgo(j.updated_at || j.created_at)}</span>, sortKey: (j) => j.updated_at || j.created_at || "" },
        ]}
      />

      <SlideOver open={!!selId} onClose={() => setSelId(null)} title={<span className="mono">{sel?.id}</span>} width={520}>
        {sel && (
          <div className="at-stack text-sm">
            <div className="at-panel">
              <div className="at-panel-bar">
                <span className="at-caption">Detail</span>
              </div>
              <div className="grid grid-cols-2 gap-px" style={{ background: "var(--at-line)" }}>
                <Kv k="Type" v={sel.job_type} mono />
                <Kv k="State" v={sel.state} />
                <Kv k="By" v={sel.requested_by} mono />
                <Kv k="Progress" v={`${sel.progress_percent ?? 0}%`} />
                <Kv k="Created" v={timeAgo(sel.created_at)} />
                <Kv k="Updated" v={timeAgo(sel.updated_at)} />
              </div>
            </div>
            {sel.error && (
              <div className="at-panel">
                <div className="at-panel-bar">
                  <span className="at-caption" style={{ color: "var(--at-fail)" }}>Error</span>
                </div>
                <div
                  className="mono text-xs text-danger"
                  style={{ padding: 12, background: "var(--at-ridge)", whiteSpace: "pre-wrap" }}
                >
                  {sel.error}
                </div>
              </div>
            )}
            <div className="at-panel">
              <div className="at-panel-bar">
                <span className="at-caption">Result</span>
              </div>
              <pre
                className="mono overflow-auto whitespace-pre-wrap"
                style={{
                  margin: 0,
                  padding: 14,
                  fontSize: 11,
                  maxHeight: "50vh",
                  background: "var(--at-ridge)",
                  color: "var(--at-ink-2)",
                }}
              >
                {sel.result ? JSON.stringify(sel.result, null, 2) : "—"}
              </pre>
            </div>
          </div>
        )}
      </SlideOver>
    </div>
  );
}

function Kv({ k, v, mono }: { k: string; v?: string; mono?: boolean }) {
  return (
    <div style={{ padding: "10px 14px", background: "var(--at-shelf)" }}>
      <div className="at-caption">{k}</div>
      <div className={mono ? "mono" : ""} style={{ marginTop: 4, color: "var(--at-ink)" }}>{v || "—"}</div>
    </div>
  );
}
