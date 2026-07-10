// Copyright (c) 2026 ZyvorAI Labs Private Limited. All rights reserved.
// Day-2 cross-cluster DR: mirroring peers, mirrored images, and failover (promote/demote).
import { useState, type ReactNode } from "react";
import { GitBranch } from "lucide-react";
import { submit } from "../api/client";
import { useDrMirrors, useDrPeers, useDrStatus, useInvalidate } from "../api/hooks";
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
  const [peerName, setPeerName] = useState("");
  const refresh = () => inv("dr-status", "dr-peers", "dr-mirrors");

  return (
    <div>
      <PageHeader icon={GitBranch} title="Disaster Recovery" subtitle="Cross-cluster RBD mirroring & failover (scaffolding — real mirroring needs a peer cluster)" />

      <div className="mb-4 grid grid-cols-2 gap-3 sm:grid-cols-5">
        <Stat label="Peers" value={status?.peers ?? "—"} />
        <Stat label="Mirrors" value={status?.mirrors ?? "—"} />
        <Stat label="Primary" value={status?.primary ?? "—"} />
        <Stat label="Secondary" value={status?.secondary ?? "—"} />
        <Stat label="Worst RPO" value={status?.worst_rpo_seconds != null ? `${status.worst_rpo_seconds}s` : "—"} />
      </div>

      <GlassSection title="Mirroring peers"
        actions={
          <div className="flex items-end gap-2">
            <Field className="w-40" value={peerName} onChange={(e) => setPeerName(e.target.value)} placeholder="peer name (dc2)" />
            <Button disabled={!peerName.trim()} onClick={() => submit("post", "/dr/peers", { name: peerName.trim() }, "peer registered", () => { setPeerName(""); refresh(); })}>Register</Button>
          </div>
        }>
        <Table rows={peers} rowKey={(p) => p.id}
          cols={[
            { h: "Name", f: (p) => p.name },
            { h: "Cluster FSID", f: (p) => p.cluster_fsid || "—", mono: true },
            { h: "Direction", f: (p) => <Badge kind="info">{p.direction}</Badge> },
            { h: "State", f: (p) => p.state, mono: true },
          ]} />
      </GlassSection>

      <GlassSection title={<>Mirrored images <Badge kind="neutral">{mirrors?.length ?? 0}</Badge></>}>
        <Table rows={mirrors} rowKey={(m) => m.id}
          cols={[
            { h: "Image", f: (m) => `${m.pool}/${m.image}`, mono: true },
            { h: "Role", f: (m) => <Badge kind={m.role === "primary" ? "success" : "info"} dot>{m.role}</Badge> },
            { h: "State", f: (m) => m.state, mono: true },
            { h: "Mode", f: (m) => m.mode },
            { h: "RPO", f: (m) => (m.rpo_seconds != null ? `${m.rpo_seconds}s` : "—") },
          ]}
          actions={(m) =>
            m.role === "secondary" ? (
              <Button size="sm" onClick={() => confirmThen({ title: "Fail over (promote)?", message: `Promote ${m.pool}/${m.image} to primary on this cluster.`, confirmLabel: "Promote" }, () => submit("post", `/dr/mirrors/${m.id}/promote`, null, "promoted", refresh))}>Promote</Button>
            ) : (
              <Button size="sm" variant="secondary" onClick={() => confirmThen({ title: "Demote to secondary?", message: `${m.pool}/${m.image}`, confirmLabel: "Demote" }, () => submit("post", `/dr/mirrors/${m.id}/demote`, null, "demoted", refresh))}>Demote</Button>
            )
          } />
      </GlassSection>
    </div>
  );
}
