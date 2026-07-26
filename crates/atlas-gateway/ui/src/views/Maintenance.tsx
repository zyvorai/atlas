// Copyright (c) 2026 ZyvorAI Labs Private Limited. All rights reserved.
// Day-2 maintenance: upgrade pre-flight, job-engine pause, backend cordon, orphan backups.
import { AlertTriangle, CheckCircle2, PauseCircle, PlayCircle, Wrench } from "lucide-react";
import { submit } from "../api/client";
import { useBackends, useInvalidate, useMaintenance, useOrphans, usePreflight } from "../api/hooks";
import { Badge, Button, GlassSection, PageHeader } from "../ui/kit";
import { Table } from "../ui/Table";
import { confirmThen } from "../ui/confirm";
import { timeAgo } from "../lib/format";

export default function Maintenance() {
  const inv = useInvalidate();
  const { data: pre } = usePreflight();
  const { data: maint } = useMaintenance();
  const { data: backends } = useBackends();
  const { data: orphans } = useOrphans();
  const paused = !!maint?.paused;

  return (
    <div>
      <PageHeader icon={Wrench} title="Maintenance" subtitle="Upgrade readiness, job-engine pause, backend cordon, and orphan cleanup" />

      {/* Upgrade pre-flight gate */}
      <GlassSection title={<>Upgrade pre-flight {pre && <Badge kind={pre.ready ? "success" : "warning"} dot>{pre.ready ? "ready" : "blocked"}</Badge>}</>}>
        {pre?.blockers?.length ? (
          <div className="mb-2 flex items-start gap-2 text-warning">
            <AlertTriangle size={16} className="mt-0.5" />
            <div>{pre.blockers.map((b, i) => <div key={i}>{b}</div>)}</div>
          </div>
        ) : (
          <div className="mb-2 flex items-center gap-2 text-success"><CheckCircle2 size={16} /> Safe to upgrade.</div>
        )}
        <Table rows={pre?.checks} rowKey={(c) => c.check}
          cols={[
            { h: "Check", f: (c) => c.check, mono: true },
            { h: "OK", f: (c) => <Badge kind={c.ok ? "success" : "warning"} dot>{c.ok ? "ok" : "blocked"}</Badge> },
            { h: "Detail", f: (c) => <span className="text-muted-foreground">{c.detail}</span> },
          ]} />
      </GlassSection>

      {/* Job-engine pause */}
      <GlassSection title={<>Job engine {maint && <Badge kind={paused ? "warning" : "success"} dot>{paused ? "paused" : "running"}</Badge>}</>}
        actions={
          paused ? (
            <Button icon={PlayCircle} onClick={() => submit("post", "/maintenance", { paused: false }, "resumed", () => inv("maintenance")).catch(() => {})}>Resume</Button>
          ) : (
            <Button icon={PauseCircle} variant="secondary" onClick={() => submit("post", "/maintenance", { paused: true }, "paused", () => inv("maintenance")).catch(() => {})}>Pause</Button>
          )
        }>
        <p className="text-sm text-muted-foreground">
          While paused, new jobs stay <span className="mono">queued</span> and drain when resumed — quiesce the system before maintenance.
        </p>
      </GlassSection>

      {/* Backend cordon */}
      <GlassSection title="Backends">
        <Table rows={backends} rowKey={(b) => b.id}
          cols={[
            { h: "Name", f: (b) => b.name },
            { h: "Type", f: (b) => <Badge kind="info">{b.backend_type}</Badge> },
            { h: "Status", f: (b) => b.status, mono: true },
            { h: "Provisioning", f: (b) => <Badge kind={b.cordoned ? "warning" : "success"} dot>{b.cordoned ? "cordoned" : "open"}</Badge> },
          ]}
          actions={(b) =>
            b.cordoned ? (
              <Button size="sm" onClick={() => submit("post", `/backends/${b.id}/uncordon`, null, "uncordoned", () => inv("backends")).catch(() => {})}>Uncordon</Button>
            ) : (
              <Button size="sm" variant="secondary" onClick={() => confirmThen({ title: "Cordon backend?", message: `${b.name} will reject new provisioning (existing volumes untouched).`, confirmLabel: "Cordon" }, () => submit("post", `/backends/${b.id}/cordon`, null, "cordoned", () => inv("backends")))}>Cordon</Button>
            )
          } />
      </GlassSection>

      {/* Orphan backups */}
      <GlassSection title={<>Orphan backups <Badge kind={orphans?.count ? "warning" : "neutral"}>{orphans?.count ?? 0}</Badge></>}>
        <Table rows={orphans?.orphan_backups} rowKey={(b) => b.id}
          cols={[
            { h: "Backup", f: (b) => b.id, mono: true },
            { h: "Source volume (gone)", f: (b) => b.volume_id, mono: true },
            { h: "Object key", f: (b) => <span className="text-muted-foreground">{b.object_key}</span> },
            { h: "Since", f: (b) => <span className="text-muted-foreground">{timeAgo(b.created_at)}</span> },
          ]}
          actions={(b) => (
            <Button size="sm" variant="danger" onClick={() => confirmThen({ title: "Delete orphan backup?", message: b.id, confirmLabel: "Delete", danger: true }, () => submit("delete", `/backups/${b.id}`, null, "backup deleted", () => inv("orphans")))}>Delete</Button>
          )} />
      </GlassSection>
    </div>
  );
}
