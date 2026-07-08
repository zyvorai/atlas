// Copyright (c) 2026 ZyvorAI Labs Private Limited. All rights reserved.
import { useState } from "react";
import { Clock } from "lucide-react";
import { useJobs } from "../api/hooks";
import { useUi } from "../store/ui";
import type { JobRecord } from "../api/types";
import { Badge, Copyable, GlassSection, PageHeader, SlideOver } from "../ui/kit";
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
  const [sel, setSel] = useState<JobRecord | null>(null);
  return (
    <div>
      <PageHeader icon={Clock} title="Jobs" subtitle="Async storage operations — live progress via server-sent events" />
      {live.length > 0 && (
        <GlassSection title="Live (this session)" className="mb-4">
          <Table
            rows={live}
            rowKey={(j) => j.id}
            cols={[
              { h: "Job", f: (j) => j.label },
              { h: "State", f: (j) => <Badge kind={stateKind(j.state)} dot>{j.state}</Badge> },
              { h: "Progress", f: (j) => <Bar pct={j.progress} kind={j.state} /> },
              { h: "ID", f: (j) => j.id, mono: true },
            ]}
          />
        </GlassSection>
      )}
      <GlassSection title="Recent jobs">
        <Table
          rows={data}
          rowKey={(j) => j.id}
          onRow={setSel}
          cols={[
            { h: "ID", f: (j) => <Copyable text={j.id} />, mono: true },
            { h: "Type", f: (j) => j.job_type, mono: true, sortKey: (j) => j.job_type },
            { h: "State", f: (j) => <Badge kind={stateKind(j.state)} dot>{j.state}</Badge>, sortKey: (j) => j.state },
            { h: "Progress", f: (j) => <Bar pct={j.progress_percent} kind={j.state} /> },
            { h: "Error", f: (j) => <span className="text-danger">{j.error || ""}</span> },
            { h: "When", f: (j) => <span className="text-muted-foreground">{timeAgo(j.updated_at || j.created_at)}</span>, sortKey: (j) => j.updated_at || j.created_at || "" },
          ]}
        />
      </GlassSection>

      <SlideOver open={!!sel} onClose={() => setSel(null)} title={<span className="mono">{sel?.id}</span>} width={520}>
        {sel && (
          <div className="space-y-4 text-sm">
            <div className="grid grid-cols-2 gap-2">
              <Kv k="Type" v={sel.job_type} mono />
              <Kv k="State" v={sel.state} />
              <Kv k="By" v={sel.requested_by} mono />
              <Kv k="Progress" v={`${sel.progress_percent ?? 0}%`} />
              <Kv k="Created" v={timeAgo(sel.created_at)} />
              <Kv k="Updated" v={timeAgo(sel.updated_at)} />
            </div>
            {sel.error && (
              <div>
                <div className="section-label mb-1">Error</div>
                <div className="glass-card p-2 text-danger text-xs mono">{sel.error}</div>
              </div>
            )}
            <div>
              <div className="section-label mb-1">Result</div>
              <pre className="glass-card p-3 text-[11px] mono overflow-auto max-h-[50vh] whitespace-pre-wrap">
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
    <div>
      <div className="section-label">{k}</div>
      <div className={mono ? "mono" : ""}>{v || "—"}</div>
    </div>
  );
}
