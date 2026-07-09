// Copyright (c) 2026 ZyvorAI Labs Private Limited. All rights reserved.
import { useState } from "react";
import { ShieldCheck } from "lucide-react";
import { useValidations } from "../../api/hooks";
import type { ValidationRun } from "../../api/types";
import { Badge, Button, GlassSection, PageHeader, SlideOver } from "../../ui/kit";
import { Table } from "../../ui/Table";

export default function Validation() {
  const { data } = useValidations();
  const [detail, setDetail] = useState<ValidationRun | null>(null);
  return (
    <div>
      <PageHeader icon={ShieldCheck} title="Validation" subtitle="Source vs edge comparisons (row counts, checksums, schema diff) run before cutover" />
      <GlassSection title={<>Validation runs <Badge kind="neutral">{data?.length || 0}</Badge></>}>
        <Table
          rows={data}
          rowKey={(r) => r.id}
          empty="No validation runs yet."
          cols={[
            { h: "Kind", f: (r) => r.kind },
            { h: "Tables", f: (r) => `${r.tables_total - r.tables_mismatched}/${r.tables_total} match` },
            { h: "State", f: (r) => <Badge kind={r.state === "passed" ? "success" : r.state === "failed" ? "danger" : "warning"} dot>{r.state}</Badge> },
            { h: "When", f: (r) => r.completed_at || r.created_at || "—" },
          ]}
          actions={(r) => <Button size="sm" onClick={() => setDetail(r)}>Details</Button>}
        />
      </GlassSection>

      <SlideOver open={!!detail} onClose={() => setDetail(null)} title={<span className="mono">{detail?.kind} validation</span>} width={560}>
        {detail && (
          <Table rows={(detail.summary?.tables || []) as any[]} rowKey={(t: any) => t.table}
            cols={[
              { h: "Table", f: (t: any) => t.table, mono: true },
              { h: "Source rows", f: (t: any) => t.source_rows?.toLocaleString?.() ?? t.source_rows },
              { h: "Edge rows", f: (t: any) => t.edge_rows?.toLocaleString?.() ?? t.edge_rows },
              { h: "Checksum", f: (t: any) => t.checksum_match ? <Badge kind="success">match</Badge> : <Badge kind="danger">mismatch</Badge> },
            ]} empty="No per-table results." />
        )}
      </SlideOver>
    </div>
  );
}
