// Copyright (c) 2026 ZyvorAI Labs Private Limited. All rights reserved.
import { Timer } from "lucide-react";
import { submit } from "../api/client";
import { useInvalidate, useSchedules } from "../api/hooks";
import { Badge, Button, GlassSection, PageHeader } from "../ui/kit";
import { Table } from "../ui/Table";
import { timeAgo } from "../lib/format";

export default function Schedules() {
  const { data } = useSchedules();
  const inv = useInvalidate();
  return (
    <div>
      <PageHeader icon={Timer} title="Schedules" subtitle="Protection schedules — periodic snapshots & backups. Create one from a volume." />
      <GlassSection title={<>Schedules <Badge kind="neutral">{data?.length || 0}</Badge></>}>
        <Table
          rows={data}
          rowKey={(s) => s.id}
          cols={[
            { h: "ID", f: (s) => s.id, mono: true },
            { h: "Kind", f: (s) => <Badge kind={s.kind === "backup" ? "info" : "neutral"}>{s.kind}</Badge> },
            { h: "Volume", f: (s) => s.volume_id, mono: true },
            { h: "Every", f: (s) => `${s.interval_secs}s` },
            { h: "Keep", f: (s) => s.keep },
            { h: "Bucket", f: (s) => s.bucket_id || "—", mono: true },
            { h: "Next run", f: (s) => <span className="text-muted-foreground">{timeAgo(s.next_run_at)}</span> },
          ]}
          actions={(s) => (
            <Button size="sm" variant="danger" onClick={() => submit("delete", `/schedules/${s.id}`, null, "delete schedule", () => inv("schedules"))}>Del</Button>
          )}
        />
      </GlassSection>
    </div>
  );
}
