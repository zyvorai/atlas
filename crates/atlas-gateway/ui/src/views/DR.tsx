// Copyright (c) 2026 ZyvorAI Labs Private Limited. All rights reserved.
// Day-2 cross-cluster DR: mirroring peers, mirrored images, and failover (promote/demote).
import { useState } from "react";
import { submit, submitJob } from "../api/client";
import { useDrMirrors, useDrPeers, useDrPreflight, useDrStatus, useInvalidate } from "../api/hooks";
import { Badge, Button, Field } from "../ui/kit";
import { PageHead } from "../ui/PageHead";
import { Table } from "../ui/Table";
import { confirmThen } from "../ui/confirm";

export default function DR() {
  const inv = useInvalidate();
  const { data: status } = useDrStatus();
  const { data: peers } = useDrPeers();
  const { data: mirrors } = useDrMirrors();
  const { data: preflight } = useDrPreflight();
  const [peerName, setPeerName] = useState("");
  const refresh = () => inv("dr-status", "dr-peers", "dr-mirrors", "dr-preflight");

  return (
    <div className="at-stack">
      <PageHead
        eyebrow="INFRASTRUCTURE · OPS"
        title="Disaster Recovery"
        state={
          status
            ? `${status.peers ?? 0} peer${(status.peers ?? 0) === 1 ? "" : "s"} · ${status.mirrors ?? 0} mirror${(status.mirrors ?? 0) === 1 ? "" : "s"} — RBD mirroring & failover (docs/DR.md).`
            : "Cross-cluster RBD mirroring & failover — control plane ready; live mirror needs a peer cluster."
        }
      />

      <div className="grid grid-cols-2 gap-3 sm:grid-cols-5" style={{ marginBottom: 0 }}>
        <div className="at-instr" style={{ border: "1px solid var(--at-line)", borderRadius: "var(--r-panel)" }}>
          <div className="at-caption">Peers</div>
          <div className="at-val md">{status?.peers ?? "—"}</div>
        </div>
        <div className="at-instr" style={{ border: "1px solid var(--at-line)", borderRadius: "var(--r-panel)" }}>
          <div className="at-caption">Mirrors</div>
          <div className="at-val md">{status?.mirrors ?? "—"}</div>
        </div>
        <div className="at-instr" style={{ border: "1px solid var(--at-line)", borderRadius: "var(--r-panel)" }}>
          <div className="at-caption">Primary</div>
          <div className="at-val md">{status?.primary ?? "—"}</div>
        </div>
        <div className="at-instr" style={{ border: "1px solid var(--at-line)", borderRadius: "var(--r-panel)" }}>
          <div className="at-caption">Secondary</div>
          <div className="at-val md">{status?.secondary ?? "—"}</div>
        </div>
        <div className="at-instr" style={{ border: "1px solid var(--at-line)", borderRadius: "var(--r-panel)" }}>
          <div className="at-caption">Worst RPO</div>
          <div className="at-val md mono">
            {status?.worst_rpo_seconds != null ? `${status.worst_rpo_seconds}s` : "—"}
          </div>
        </div>
      </div>

      <div className="at-panel">
        <div className="at-panel-bar">
          <span className="at-caption">Preflight</span>
          <span className="grow" />
          <Badge kind={preflight?.ready ? "success" : "warning"}>
            {preflight?.ready ? "ready" : "blocked"}
          </Badge>
        </div>
        <div className="at-list-row">
          <span className="at-sub" style={{ margin: 0 }}>
            {status?.note || "Run before failover drills"}
          </span>
        </div>
        {(preflight?.checks ?? []).map((c: any) => (
          <div key={c.id} className="at-list-row" style={{ alignItems: "center" }}>
            <Badge kind={c.ok ? "success" : "danger"}>{c.ok ? "ok" : "fail"}</Badge>
            <span className="mono" style={{ fontSize: 12 }}>{c.id}</span>
            <span className="at-sub" style={{ margin: 0, flex: 1 }}>{c.detail}</span>
          </div>
        ))}
        {(preflight?.checks ?? []).length === 0 && (
          <div className="at-list-row">
            <span className="at-sub" style={{ margin: 0 }}>No preflight checks yet.</span>
          </div>
        )}
      </div>

      <Table
        soundings
        panelTitle="Mirroring peers"
        panelExtra={
          <div className="flex items-center gap-2">
            <Field
              className="w-40"
              value={peerName}
              onChange={(e) => setPeerName(e.target.value)}
              placeholder="peer name (dc2)"
            />
            <button
              type="button"
              className="at-btn primary"
              style={{ height: 28 }}
              disabled={!peerName.trim()}
              onClick={() => submit("post", "/dr/peers", { name: peerName.trim() }, "peer registered", () => { setPeerName(""); refresh(); }).catch(() => {})}
            >
              Register
            </button>
          </div>
        }
        rows={peers}
        rowKey={(p) => p.id}
        cols={[
          { h: "Name", f: (p) => p.name },
          { h: "Cluster FSID", f: (p) => p.cluster_fsid || "—", mono: true },
          { h: "Direction", f: (p) => <Badge kind="info">{p.direction}</Badge> },
          { h: "State", f: (p) => p.state, mono: true },
        ]}
        actions={(p) => (
          <Button size="sm" variant="secondary" onClick={() => confirmThen({ title: "Delete peer?", message: p.name, confirmLabel: "Delete" }, () => submit("delete", `/dr/peers/${p.id}`, null, "peer deleted", refresh))}>Delete</Button>
        )}
      />

      <Table
        soundings
        panelTitle={
          <>
            Mirrored images <Badge kind="neutral">{mirrors?.length ?? 0}</Badge>
          </>
        }
        rows={mirrors}
        rowKey={(m) => m.id}
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
        )}
      />
    </div>
  );
}
