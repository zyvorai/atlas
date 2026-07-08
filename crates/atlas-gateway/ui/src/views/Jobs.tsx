// Copyright (c) 2026 ZyvorAI Labs Private Limited. All rights reserved.
import { Clock } from "lucide-react";
import { useJobs } from "../api/hooks";
import { useUi } from "../store/ui";
import { Badge, Copyable, GlassSection, PageHeader } from "../ui/kit";
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
    </div>
  );
}
