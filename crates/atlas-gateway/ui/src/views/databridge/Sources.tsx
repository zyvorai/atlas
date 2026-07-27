// Copyright (c) 2026 ZyvorAI Labs Private Limited. All rights reserved.
import { useState } from "react";
import { Database, Plus, Search } from "lucide-react";
import { http, submitJob, submit, toast } from "../../api/client";
import { useSources, useInvalidate } from "../../api/hooks";
import type { MigrationSource } from "../../api/types";
import { Badge, Button, FormModal, GlassSection, PageHeader, SlideOver } from "../../ui/kit";
import { del } from "../../ui/confirm";
import { Table } from "../../ui/Table";
import { fmtBytes } from "../../lib/format";

const stateKind = (s: string) =>
  s === "discovered" ? "success" : s === "error" ? "danger" : s === "discovering" ? "warning" : "neutral";

export default function Sources() {
  const { data } = useSources();
  const inv = useInvalidate();
  const refetch = () => inv("db-sources");
  const [create, setCreate] = useState(false);
  const [detail, setDetail] = useState<MigrationSource | null>(null);

  return (
    <div>
      <PageHeader icon={Database} title="Cloud Databases" subtitle="Registered source databases (AWS RDS/Aurora, GCP Cloud SQL) — discover schema before migrating"
        actions={<Button variant="primary" icon={Plus} onClick={() => setCreate(true)}>Register source</Button>} />
      <GlassSection title={<>Sources <Badge kind="neutral">{data?.length || 0}</Badge></>}>
        <Table
          rows={data}
          rowKey={(r) => r.id}
          empty="No sources yet."
          emptyCta={<Button variant="primary" icon={Plus} onClick={() => setCreate(true)}>Register source</Button>}
          cols={[
            { h: "Name", f: (r) => r.name, mono: true },
            { h: "Engine", f: (r) => <Badge kind="info">{r.kind}</Badge> },
            { h: "Cloud", f: (r) => r.cloud },
            { h: "Endpoint", f: (r) => <span className="mono text-muted-foreground">{r.endpoint || "—"}</span> },
            { h: "Tables", f: (r) => r.discovered?.tables?.length ?? "—" },
            { h: "State", f: (r) => <Badge kind={stateKind(r.state)} dot>{r.state}</Badge> },
          ]}
          actions={(r) => (
            <>
              <Button size="sm" icon={Search} onClick={() => submitJob("post", `/databridge/sources/${r.id}/discover`, null, "discover source", refetch).catch(() => {})}>Discover</Button>
              <Button size="sm" onClick={() => setDetail(r)}>Schema</Button>
              <Button size="sm" variant="danger" onClick={() => del(`source ${r.name}`, async () => { await submit("delete", `/databridge/sources/${r.id}`, null, "delete source"); refetch(); })}>Del</Button>
            </>
          )}
        />
      </GlassSection>

      <FormModal open={create} onClose={() => setCreate(false)} title="Register source database" submitLabel="Register"
        fields={[
          { name: "name", label: "Name" },
          {
            name: "kind", label: "Engine",
            options: ["postgres", "mysql", "mariadb", "oracle", "sqlserver", "mongodb"].map((v) => ({ value: v, label: v })),
          },
          {
            name: "cloud", label: "Cloud",
            options: ["rds", "aurora", "cloudsql", "generic"].map((v) => ({ value: v, label: v })),
          },
          { name: "endpoint", label: "Endpoint host", optional: true },
          { name: "port", label: "Port", type: "number", optional: true, min: 1 },
          { name: "database", label: "Database", optional: true },
          { name: "driver_mode", label: "Driver mode", options: [{ value: "fake", label: "fake" }, { value: "real", label: "real" }] },
        ]}
        onSubmit={(v) => {
          const body: Record<string, unknown> = { name: v.name, kind: v.kind, cloud: v.cloud, driver_mode: v.driver_mode };
          if (v.endpoint) body.endpoint = v.endpoint;
          if (v.port) body.port = +v.port;
          if (v.database) body.database = v.database;
          return submit("post", "/databridge/sources", body, "register source", refetch);
        }} />

      <SchemaBrowser source={detail} onClose={() => setDetail(null)} />
    </div>
  );
}

function SchemaBrowser({ source, onClose }: { source: MigrationSource | null; onClose: () => void }) {
  if (!source) return null;
  const d = source.discovered;
  return (
    <SlideOver open={!!source} onClose={onClose} title={<span className="mono">{source.name} · schema</span>} width={620}>
      {!d || !d.tables?.length ? (
        <div className="text-muted-foreground text-sm">
          No schema discovered yet. Run <b>Discover</b> on this source first.
        </div>
      ) : (
        <>
          <div className="flex gap-4 mb-3 text-sm">
            <span>Engine <b>{d.engine} {d.version}</b></span>
            <span>Size <b>{fmtBytes(d.total_size_bytes || 0)}</b></span>
            <span>CDC {d.cdc_capable ? <Badge kind="success">capable</Badge> : <Badge kind="danger">off</Badge>}</span>
          </div>
          <Table rows={d.tables} rowKey={(t) => `${t.schema}.${t.name}`}
            cols={[
              { h: "Table", f: (t) => `${t.schema}.${t.name}`, mono: true },
              { h: "Rows", f: (t) => t.est_rows.toLocaleString() },
              { h: "Size", f: (t) => fmtBytes(t.size_bytes) },
              { h: "PK", f: (t) => t.has_primary_key ? <Badge kind="success">yes</Badge> : <Badge kind="warning">none</Badge> },
            ]} empty="No tables." />
        </>
      )}
    </SlideOver>
  );
}
