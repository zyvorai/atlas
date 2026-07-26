// Copyright (c) 2026 ZyvorAI Labs Private Limited. All rights reserved.
import { useState } from "react";
import { Activity, PlayCircle } from "lucide-react";
import { submit } from "../api/client";
import { useAlerts, useInvalidate } from "../api/hooks";
import { Badge, Button, GlassSection, PageHeader, Select } from "../ui/kit";
import { Table } from "../ui/Table";
import { confirmThen } from "../ui/confirm";
import { stateKind, timeAgo } from "../lib/format";

export default function Alerts() {
  const [state, setState] = useState("");
  const { data } = useAlerts(state || undefined);
  const inv = useInvalidate();
  return (
    <div>
      <PageHeader icon={Activity} title="Alerts" subtitle="Cluster health, pool near-full, OSD down/latency, recovery in progress"
        actions={<Button icon={PlayCircle} onClick={() => submit("post", "/alerts/evaluate", null, "evaluated", () => inv("alerts")).catch(() => {})}>Evaluate</Button>} />
      <div className="mb-3">
        <Select value={state} onChange={(e) => setState(e.target.value)} className="w-40">
          <option value="">All</option><option value="open">Open</option><option value="resolved">Resolved</option>
        </Select>
      </div>
      <GlassSection title={<>Alerts <Badge kind="neutral">{data?.length || 0}</Badge></>}>
        <Table
          rows={data}
          rowKey={(a) => a.id}
          cols={[
            { h: "Severity", f: (a) => <Badge kind={stateKind(a.severity)} dot>{a.severity}</Badge> },
            { h: "State", f: (a) => <Badge kind={a.state === "open" ? "warning" : "neutral"}>{a.state}</Badge> },
            { h: "Title", f: (a) => a.title },
            { h: "Detail", f: (a) => <span className="text-muted-foreground">{a.description}</span> },
            { h: "Resource", f: (a) => a.resource_id, mono: true },
            { h: "Since", f: (a) => <span className="text-muted-foreground">{timeAgo(a.created_at)}</span> },
          ]}
          actions={(a) =>
            a.state === "open" ? (
              <>
                <Button size="sm" onClick={() => submit("post", `/alerts/${a.id}/ack`, null, "acknowledged", () => inv("alerts")).catch(() => {})}>Ack</Button>
                <Button size="sm" onClick={() => submit("post", `/alerts/${a.id}/silence?secs=3600`, null, "silenced 1h", () => inv("alerts")).catch(() => {})}>Silence</Button>
                <Button size="sm" variant="danger" onClick={() => confirmThen({ title: "Resolve alert?", message: a.title, confirmLabel: "Resolve" }, () => submit("post", `/alerts/${a.id}/resolve`, null, "resolved", () => inv("alerts")))}>Resolve</Button>
              </>
            ) : null
          }
        />
      </GlassSection>
    </div>
  );
}
