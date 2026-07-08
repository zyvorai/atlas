// Copyright (c) 2026 ZyvorAI Labs Private Limited. All rights reserved.
import { useState } from "react";
import { FileClock } from "lucide-react";
import { useAudit } from "../api/hooks";
import { Badge, GlassSection, PageHeader } from "../ui/kit";
import { Table } from "../ui/Table";
import { stateKind, timeAgo } from "../lib/format";

export default function Audit() {
  const [actor, setActor] = useState("");
  const [action, setAction] = useState("");
  const qs = new URLSearchParams({ limit: "80" });
  if (actor) qs.set("actor", actor);
  if (action) qs.set("action", action);
  const { data } = useAudit(`?${qs.toString()}`);
  return (
    <div>
      <PageHeader icon={FileClock} title="Audit" subtitle="Compliance trail of state-changing and sensitive actions" />
      <div className="flex gap-2 mb-3">
        <input className="field w-48" placeholder="Filter actor…" value={actor} onChange={(e) => setActor(e.target.value)} />
        <input className="field w-56" placeholder="Filter action…" value={action} onChange={(e) => setAction(e.target.value)} />
      </div>
      <GlassSection title={<>Audit trail <Badge kind="neutral">{data?.length || 0}</Badge></>}>
        <Table
          rows={data}
          rowKey={(a) => String(a.id)}
          cols={[
            { h: "When", f: (a) => <span className="text-muted-foreground">{timeAgo(a.created_at)}</span> },
            { h: "Actor", f: (a) => a.actor_id, mono: true },
            { h: "Action", f: (a) => a.action, mono: true },
            { h: "Resource", f: (a) => <span className="mono">{a.resource_type}/{a.resource_id}</span> },
            { h: "Status", f: (a) => <Badge kind={stateKind(a.status)}>{a.status}</Badge> },
          ]}
        />
      </GlassSection>
    </div>
  );
}
