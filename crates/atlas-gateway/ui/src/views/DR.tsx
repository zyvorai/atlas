// Copyright (c) 2026 ZyvorAI Labs Private Limited. All rights reserved.
// Day-2 cross-cluster DR: mirroring peers, mirrored images, and failover (promote/demote).
import { useState, type ReactNode } from "react";
import { GitBranch } from "lucide-react";
import { submit, submitJob } from "../api/client";
import { useDrMirrors, useDrPeers, useDrPreflight, useDrStatus, useInvalidate } from "../api/hooks";
import { Badge, Button, Field, GlassSection, PageHeader } from "../ui/kit";
import { Table } from "../ui/Table";
import { confirmThen } from "../ui/confirm";

function Stat({ label, value }: { label: string; value: ReactNode }) {
  return (
    <div className="glass-card px-4 py-3">
      <div className="section-label">{label}</div>
      <div className="text-2xl font-semibold">{value}</div>
    </div>
  );
}

export default function DR() {
  const inv = useInvalidate();
  const { data: status } = useDrStatus();
  const { data: peers } = useDrPeers();
  const { data: mirrors } = useDrMirrors();
  const { data: preflight } = useDrPreflight();
  const [peerName, setPeerName] = useState("");
  const refresh = () => inv("dr-status", "dr-peers", "dr-mirrors", "dr-preflight");

  return (
    <div>
      <PageHeader
        icon={GitBranch}
        title="Disaster Recovery"
        subtitle="Cross-cluster RBD mirroring & failover — control plane ready; live rbd mirror needs a peer cluster (docs/DR.md)"
      />

      <div className="mb-4 grid grid-cols-2 gap-3 sm:grid-cols-5">
        <Stat label="Peers" value={status?.peers ?? "—"} />
        <Stat label="Mirrors" value={status?.mirrors ?? "—"} />
        <Stat label="Primary" value={status?.primary ?? "—"} />
        <Stat label="Secondary" value={status?.secondary ?? "—"} />
        <Stat label="Worst RPO" value={status?.worst_rpo_seconds != null ? `${status.worst_rpo_seconds}s` : "—"} />
      </div>

      <GlassSection title="Preflight">
        <div className="mb-2 flex items-center gap-2 text-sm">
          <Badge kind={preflight?.ready ? "success" : "warning"}>{preflight?.ready ? "ready" : "blocked"}</Badge>
          <span className="text-muted">{status?.note || "Run before failover drills"}</span>
        </div>
        <ul className="space-y-1 text-sm">
          {(preflight?.checks ?? []).map((c: any) => (
            <li key={c.id} className="flex gap-2">
              <Badge kind={c.ok ? "success" : "danger"}>{c.ok ? "ok" : "fail"}</Badge>
              <span className="font-mono text-xs">{c.id}</span>
              <span className="text-muted">{c.detail}</span>
            </li>
          ))}
        </ul>
      </GlassSection>

      <GlassSection title="Mirroring peers"
        actions={
          <div className="flex items-end gap-2">
            <Field className="w-40" value={peerName} onChange={(e) => setPeerName(e.target.value)} placeholder="peer name (dc2)" />
            <Button disabled={!peerName.trim()} onClick={() => submit("post", "/dr/peers", { name: peerName.trim() }, "peer registered", () => { setPeerName(""); refresh(); }).catch(() => {})}>Register</Button>
          </div>
        }>
        <Table rows={peers} rowKey={(p) => p.id}
          cols={[
            { h: "Name", f: (p) => p.name },
            { h: "Cluster FSID", f: (p) => p.cluster_fsid || "—", mono: true },
            { h: "Direction", f: (p) => <Badge kind="info">{p.direction}</Badge> },
            { h: "State", f: (p) => p.state, mono: true },
          ]}
          actions={(p) => (
            <Button size="sm" variant="secondary" onClick={() => confirmThen({ title: "Delete peer?", message: p.name, confirmLabel: "Delete" }, () => submit("delete", `/dr/peers/${p.id}`, null, "peer deleted", refresh))}>Delete</Button>
          )} />
      </GlassSection>

      <GlassSection title={<>Mirrored images <Badge kind="neutral">{mirrors?.length ?? 0}</Badge></>}>
        <Table rows={mirrors} rowKey={(m) => m.id}
          cols={[
            { h: "Image", f: (m) => `${m.pool}/${m.image}`, mono: true },
            { h: "Role", f: (m) => <Badge kind={m.role === "primary" ? "success" : "info"} dot>{m.role}</Badge> },
            { h: "State", f: (m) => m.state, mono: true },
            { h: "Mode", f: (m) => m.mode },
            { h: "RPO", f: (m) => (m.rpo_seconds != null ? `${m.rpo_seconds}s` : "—") },
            { h: "Last failover", f: (m) => m.last_failover_at || "—" },
          ]}
          actions={(m) => (
            <div className="flex flex-wrap gap-1">
              {m.role === "secondary" ? (
                <>
                  <Button size="sm" onClick={() => confirmThen({ title: "Fail over (promote)?", message: `Promote ${m.pool}/${m.image} to primary on this cluster.`, confirmLabel: "Promote" }, () => submitJob("post", `/dr/mirrors/${m.id}/promote`, null, "promote", refresh))}>Promote</Button>
                  <Button size="sm" variant="secondary" onClick={() => confirmThen({ title: "Force promote (split-brain)?", message: `${m.pool}/${m.image}`, confirmLabel: "Force" }, () => submitJob("post", `/dr/mirrors/${m.id}/promote?force=true`, null, "force-promote", refresh))}>Force</Button>
                  <Button size="sm" variant="primary" onClick={() => confirmThen({ title: "Confirm-gated failover?", message: `POST /dr/failover for ${m.pool}/${m.image}. Requires confirm=true.`, confirmLabel: "Failover" }, () => submitJob("post", "/dr/failover", { mirror_id: m.id, confirm: true }, "failover", refresh))}>Failover</Button>
                </>
              ) : (
                <Button size="sm" variant="secondary" onClick={() => confirmThen({ title: "Demote to secondary?", message: `${m.pool}/${m.image}`, confirmLabel: "Demote" }, () => submitJob("post", `/dr/mirrors/${m.id}/demote`, null, "demote", refresh))}>Demote</Button>
              )}
              <Button size="sm" variant="secondary" onClick={() => {
                const raw = window.prompt("Observed RPO seconds", m.rpo_seconds != null ? String(m.rpo_seconds) : "60");
                if (raw == null || raw.trim() === "") return;
                const rpo_seconds = Number(raw);
                if (!Number.isFinite(rpo_seconds) || rpo_seconds < 0) return;
                submit("post", `/dr/mirrors/${m.id}/rpo`, { rpo_seconds }, "rpo recorded", refresh).catch(() => {});
              }}>Set RPO</Button>
            </div>
          )} />
      </GlassSection>
    </div>
  );
}
