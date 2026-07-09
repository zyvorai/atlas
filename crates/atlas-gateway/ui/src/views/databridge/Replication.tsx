// Copyright (c) 2026 ZyvorAI Labs Private Limited. All rights reserved.
import { Radio } from "lucide-react";
import { useCdcStreams } from "../../api/hooks";
import { Badge, GlassSection, PageHeader } from "../../ui/kit";
import { Table } from "../../ui/Table";
import { fmtBytes } from "../../lib/format";

const kind = (s: string) =>
  s === "streaming" ? "success" : s === "error" ? "danger" : s === "stopped" ? "neutral" : "warning";
const lagKind = (secs: number) => (secs <= 0 ? "success" : secs < 10 ? "warning" : "danger");

export default function Replication() {
  const { data } = useCdcStreams();
  return (
    <div>
      <PageHeader icon={Radio} title="Replication" subtitle="Debezium CDC streams keeping edge databases in sync with their cloud sources — live lag" />
      <GlassSection title={<>CDC streams <Badge kind="neutral">{data?.length || 0}</Badge></>}>
        <Table
          rows={data}
          rowKey={(r) => r.id}
          empty="No CDC streams — start CDC from a migration plan."
          cols={[
            { h: "Connector", f: (r) => r.connector_name || r.id, mono: true },
            { h: "Engine", f: (r) => <Badge kind="info">{r.engine}</Badge> },
            { h: "State", f: (r) => <Badge kind={kind(r.state)} dot>{r.state}</Badge> },
            { h: "Lag (time)", f: (r) => <Badge kind={lagKind(r.lag_seconds)}>{r.lag_seconds}s</Badge> },
            { h: "Lag (bytes)", f: (r) => fmtBytes(r.lag_bytes) },
            { h: "Events", f: (r) => r.events_total.toLocaleString() },
          ]}
        />
      </GlassSection>
    </div>
  );
}
