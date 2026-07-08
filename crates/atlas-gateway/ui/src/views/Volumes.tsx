// Copyright (c) 2026 ZyvorAI Labs Private Limited. All rights reserved.
import { useState } from "react";
import { HardDrive, Plus, RefreshCw } from "lucide-react";
import { submit, submitJob } from "../api/client";
import { useBuckets, useInvalidate, useVolumes } from "../api/hooks";
import { http } from "../api/client";
import type { StorageVolume } from "../api/types";
import { Badge, Button, FormModal, GlassSection, PageHeader, Select, SlideOver } from "../ui/kit";
import { del } from "../ui/confirm";
import { Table } from "../ui/Table";
import { fmtBytes, gib, stateKind } from "../lib/format";

const POLICIES = ["database", "production", "development", "shared", "ai"];

export default function Volumes() {
  const [state, setState] = useState("");
  const [tenant, setTenant] = useState("");
  const { data: vols } = useVolumes(state || undefined, tenant || undefined);
  const inv = useInvalidate();
  const refetch = () => inv("volumes", "summary");
  const [createOpen, setCreateOpen] = useState(false);
  const [sel, setSel] = useState<StorageVolume | null>(null);
  const [modal, setModal] = useState<{ v: StorageVolume; kind: string } | null>(null);
  const { data: buckets } = useBuckets();

  return (
    <div>
      <PageHeader
        icon={HardDrive}
        title="Volumes"
        subtitle="Ceph-backed volumes (CSI PVCs + raw RBD)"
        actions={
          <>
            <Button icon={RefreshCw} onClick={refetch}>Refresh</Button>
            <Button variant="primary" icon={Plus} onClick={() => setCreateOpen(true)}>Volume</Button>
          </>
        }
      />

      <div className="flex gap-2 mb-3">
        <Select value={state} onChange={(e) => setState(e.target.value)} className="w-40">
          <option value="">All states</option>
          {["bound", "available", "creating", "deleting", "failed"].map((s) => <option key={s}>{s}</option>)}
        </Select>
        <input className="field w-48" placeholder="Filter tenant…" value={tenant} onChange={(e) => setTenant(e.target.value)} />
      </div>

      <GlassSection title={<>Volumes <Badge kind="neutral">{vols?.length || 0}</Badge></>}>
        <Table
          rows={vols}
          onRow={setSel}
          rowKey={(v) => v.id}
          empty="No volumes yet."
          emptyCta={<Button variant="primary" icon={Plus} onClick={() => setCreateOpen(true)}>Create volume</Button>}
          cols={[
            { h: "Name", f: (v) => v.name, mono: true, sortKey: (v) => v.name },
            { h: "Kind", f: (v) => v.kind, sortKey: (v) => v.kind },
            { h: "Size", f: (v) => fmtBytes(v.size_bytes), sortKey: (v) => v.size_bytes },
            { h: "Used", f: (v) => (v.used_bytes != null ? fmtBytes(v.used_bytes) : "—"), sortKey: (v) => v.used_bytes ?? -1 },
            { h: "State", f: (v) => <Badge kind={stateKind(v.state)} dot>{v.state}</Badge>, sortKey: (v) => v.state },
            { h: "Class", f: (v) => v.storage_class_name || "—", mono: true },
            { h: "PVC / RBD", f: (v) => <span className="mono text-muted-foreground">{v.pvc_name || v.backend_native_id || "—"}</span> },
          ]}
          actions={(v) => (
            <>
              <Button size="sm" onClick={() => setModal({ v, kind: "snapshot" })}>Snap</Button>
              <Button size="sm" onClick={() => setModal({ v, kind: "expand" })}>Expand</Button>
              <Button size="sm" onClick={() => setModal({ v, kind: "schedule" })}>Schedule</Button>
              <Button size="sm" variant="danger" onClick={() => del(`volume ${v.name}`, () => submitJob("delete", `/volumes/${v.id}?force=true`, null, `delete ${v.name}`, refetch))}>Del</Button>
            </>
          )}
        />
      </GlassSection>

      {/* Create */}
      <FormModal
        open={createOpen}
        onClose={() => setCreateOpen(false)}
        title="Create volume"
        submitLabel="Create"
        fields={[
          { name: "name", label: "Name", placeholder: "my-volume" },
          { name: "tenant_id", label: "Tenant", value: "default" },
          { name: "size_gib", label: "Size (GiB)", type: "number", value: "1" },
          { name: "policy", label: "Policy (intent)", options: POLICIES.map((p) => ({ value: p, label: p })) },
          { name: "namespace", label: "Namespace", value: "rook-ceph" },
        ]}
        onSubmit={(v) =>
          submitJob("post", "/volumes", {
            tenant_id: v.tenant_id, name: v.name, size_bytes: gib(+v.size_gib), policy: v.policy,
            kubernetes: { namespace: v.namespace },
          }, `create ${v.name}`, refetch)
        }
      />

      {/* Snapshot / Expand / Schedule */}
      {modal?.kind === "snapshot" && (
        <FormModal open onClose={() => setModal(null)} title={`Snapshot ${modal.v.name}`} submitLabel="Snapshot"
          fields={[{ name: "name", label: "Snapshot name (optional)", optional: true }]}
          onSubmit={(x) => submitJob("post", `/volumes/${modal.v.id}/snapshots`, { name: x.name || undefined }, "snapshot", refetch)} />
      )}
      {modal?.kind === "expand" && (
        <FormModal open onClose={() => setModal(null)} title={`Expand ${modal.v.name}`} submitLabel="Expand"
          fields={[{ name: "size_gib", label: "New size (GiB)", type: "number", value: String(Math.ceil(modal.v.size_bytes / 1073741824) + 1) }]}
          onSubmit={(x) => submitJob("post", `/volumes/${modal.v.id}/expand`, { new_size_bytes: gib(+x.size_gib) }, "expand", refetch)} />
      )}
      {modal?.kind === "schedule" && (
        <FormModal open onClose={() => setModal(null)} title={`Schedule for ${modal.v.name}`} submitLabel="Create schedule"
          fields={[
            { name: "kind", label: "Kind", options: [{ value: "snapshot", label: "snapshot" }, { value: "backup", label: "backup" }] },
            { name: "interval_secs", label: "Interval (seconds)", type: "number", value: "3600" },
            { name: "keep", label: "Keep", type: "number", value: "24" },
            { name: "bucket_id", label: "Bucket (backup only)", options: [{ value: "", label: "—" }, ...(buckets || []).filter((b) => b.state === "bound").map((b) => ({ value: b.id, label: b.bucket_name || b.id }))] },
            { name: "mode", label: "Mode (backup)", options: [{ value: "manifest", label: "manifest" }, { value: "data", label: "data" }] },
          ]}
          onSubmit={(x) => submit("post", `/volumes/${modal.v.id}/schedule`, {
            kind: x.kind, interval_secs: +x.interval_secs, keep: +x.keep,
            bucket_id: x.bucket_id || undefined, mode: x.mode,
          }, "schedule", () => inv("schedules"))} />
      )}

      <VolumeDrawer vol={sel} onClose={() => setSel(null)} refetch={refetch} />
    </div>
  );
}

function VolumeDrawer({ vol, onClose, refetch }: { vol: StorageVolume | null; onClose: () => void; refetch: () => void }) {
  const inv = useInvalidate();
  const [bindings, setBindings] = useState<any[] | null>(null);
  const [labels, setLabels] = useState<Record<string, string> | null>(null);
  const [lk, setLk] = useState("");
  const [lv, setLv] = useState("");
  // Lazy load when opened.
  if (vol && bindings === null) {
    http.get(`/volumes/${vol.id}/bindings`).then((r) => setBindings(r.data)).catch(() => setBindings([]));
    http.get(`/volumes/${vol.id}/labels`).then((r) => setLabels(r.data || {})).catch(() => setLabels({}));
  }
  const close = () => { setBindings(null); setLabels(null); onClose(); };
  if (!vol) return null;
  return (
    <SlideOver open={!!vol} onClose={close} title={<span className="mono">{vol.name}</span>} width={480}>
      <div className="space-y-4 text-sm">
        <div className="grid grid-cols-2 gap-2">
          <Kv k="ID" v={vol.id} mono /><Kv k="Kind" v={vol.kind} />
          <Kv k="Size" v={fmtBytes(vol.size_bytes)} /><Kv k="Used" v={vol.used_bytes != null ? fmtBytes(vol.used_bytes) : "—"} />
          <Kv k="State" v={vol.state} /><Kv k="Class" v={vol.storage_class_name || "—"} mono />
          <Kv k="Namespace" v={vol.kubernetes_namespace || "—"} /><Kv k="PVC" v={vol.pvc_name || "—"} mono />
          <Kv k="Native" v={vol.backend_native_id || "—"} mono />
        </div>

        <div>
          <div className="section-label mb-1">Labels</div>
          <div className="flex flex-wrap gap-1.5 mb-2">
            {labels && Object.entries(labels).length ? Object.entries(labels).map(([k, v]) => <Badge key={k} kind="info">{k}={String(v)}</Badge>) : <span className="text-muted-foreground">none</span>}
          </div>
          <div className="flex gap-2">
            <input className="field" placeholder="key" value={lk} onChange={(e) => setLk(e.target.value)} />
            <input className="field" placeholder="value" value={lv} onChange={(e) => setLv(e.target.value)} />
            <Button variant="primary" size="sm" disabled={!lk} onClick={async () => {
              const r = await http.put(`/volumes/${vol.id}/labels`, { [lk]: lv });
              setLabels(r.data); setLk(""); setLv(""); inv("volumes");
            }}>Set</Button>
          </div>
        </div>

        <div>
          <div className="section-label mb-1">Product bindings</div>
          {bindings && bindings.length ? (
            <div className="space-y-1">
              {bindings.map((b, i) => <div key={i} className="mono text-xs">{b.product} · {b.resource_type}/{b.resource_id} · {b.role}</div>)}
            </div>
          ) : <span className="text-muted-foreground">none</span>}
        </div>

        <div className="flex gap-2 pt-2">
          <Button variant="danger" onClick={() => del(`volume ${vol.name}`, () => { submitJob("delete", `/volumes/${vol.id}?force=true`, null, `delete ${vol.name}`, refetch); close(); })}>Delete volume</Button>
        </div>
      </div>
    </SlideOver>
  );
}

function Kv({ k, v, mono }: { k: string; v: string; mono?: boolean }) {
  return (
    <div>
      <div className="section-label">{k}</div>
      <div className={mono ? "mono" : ""}>{v}</div>
    </div>
  );
}
