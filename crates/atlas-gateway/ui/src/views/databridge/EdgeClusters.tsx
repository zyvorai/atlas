// Copyright (c) 2026 ZyvorAI Labs Private Limited. All rights reserved.
import { Server } from "lucide-react";
import { useEdgeClusters } from "../../api/hooks";
import { Badge, GlassSection, PageHeader } from "../../ui/kit";
import { Table } from "../../ui/Table";

const kind = (s: string) => (s === "ready" ? "success" : s === "degraded" ? "danger" : "warning");

export default function EdgeClusters() {
  const { data } = useEdgeClusters();
  return (
    <div>
      <PageHeader icon={Server} title="Edge DB Clusters" subtitle="Target databases (CloudNativePG / MySQL operator) provisioned on Ceph RBD storage" />
      <GlassSection title={<>Clusters <Badge kind="neutral">{data?.length || 0}</Badge></>}>
        <Table
          rows={data}
          rowKey={(r) => r.id}
          empty="No edge clusters yet — provision one from a migration plan."
          cols={[
            { h: "Name", f: (r) => r.cr_name || r.id, mono: true },
            { h: "Engine", f: (r) => <Badge kind="info">{r.engine}</Badge> },
            { h: "Operator", f: (r) => r.operator },
            { h: "Storage class", f: (r) => <span className="mono text-muted-foreground">{r.storage_class}</span> },
            { h: "Endpoint", f: (r) => <span className="mono text-muted-foreground">{r.service_endpoint || "—"}</span> },
            { h: "State", f: (r) => <Badge kind={kind(r.state)} dot>{r.state}</Badge> },
          ]}
        />
      </GlassSection>
    </div>
  );
}
