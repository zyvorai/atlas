// Copyright (c) 2026 ZyvorAI Labs Private Limited. All rights reserved.
import { useState } from "react";
import { Archive, Plus } from "lucide-react";
import { http, submitJob, toast } from "../api/client";
import { useBackups, useBuckets, useInvalidate, useVolumes } from "../api/hooks";
import type { BackupRecord } from "../api/types";
import { Badge, Button, FormModal, GlassSection, PageHeader } from "../ui/kit";
import { del } from "../ui/confirm";
import { Table } from "../ui/Table";
import { stateKind, timeAgo } from "../lib/format";

export default function Backups() {
  const { data } = useBackups();
  const { data: vols } = useVolumes();
  const { data: buckets } = useBuckets();
  const inv = useInvalidate();
  const refetch = () => inv("backups", "summary");
  const [create, setCreate] = useState(false);
  const [restore, setRestore] = useState<BackupRecord | null>(null);
  const [picked, setPicked] = useState<Set<string>>(new Set());
  const toggle = (k: string) => setPicked((s) => { const n = new Set(s); n.has(k) ? n.delete(k) : n.add(k); return n; });
  const toggleAll = (keys: string[]) => setPicked((s) => (keys.every((k) => s.has(k)) ? new Set() : new Set(keys)));
  const bulkDelete = () => del(`${picked.size} backup(s)`, () => { const ids = [...picked]; setPicked(new Set()); ids.forEach((id) => submitJob("delete", `/backups/${id}`, null, "delete backup", refetch).catch(() => {})); });

  return (
    <div>
      <PageHeader icon={Archive} title="Backups" subtitle="RBD backups to RGW (manifest + streamed data), restore & presigned download"
        actions={<Button variant="primary" icon={Plus} onClick={() => setCreate(true)}>Backup</Button>} />
      <GlassSection
        title={
          <span className="flex items-center gap-2 flex-1">
            Backups <Badge kind="neutral">{data?.length || 0}</Badge>
            {picked.size > 0 && (
              <span className="flex items-center gap-2 ml-2">
                <span className="text-xs text-sky-300">{picked.size} selected</span>
                <Button size="sm" variant="danger" onClick={bulkDelete}>Delete selected</Button>
                <Button size="sm" onClick={() => setPicked(new Set())}>Clear</Button>
              </span>
            )}
          </span>
        }
      >
        <Table
          rows={data}
          rowKey={(b) => b.id}
          selectable
          selected={picked}
          onToggle={toggle}
          onToggleAll={toggleAll}
          empty="No backups yet."
          emptyCta={<Button variant="primary" icon={Plus} onClick={() => setCreate(true)}>Back up a volume</Button>}
          cols={[
            { h: "ID", f: (b) => b.id, mono: true },
            { h: "Volume", f: (b) => b.volume_id, mono: true, sortKey: (b) => b.volume_id },
            { h: "Format", f: (b) => b.format, sortKey: (b) => b.format },
            { h: "State", f: (b) => <Badge kind={stateKind(b.state)} dot>{b.state}</Badge>, sortKey: (b) => b.state },
            { h: "Checksum", f: (b) => <span className="mono text-muted-foreground">{(b.checksum || "").slice(0, 12) || "—"}</span> },
            { h: "Created", f: (b) => <span className="text-muted-foreground">{timeAgo(b.created_at)}</span>, sortKey: (b) => b.created_at || "" },
          ]}
          actions={(b) => (
            <>
              <Button size="sm" onClick={() => setRestore(b)}>Restore</Button>
              <Button size="sm" onClick={async () => {
                try { const r = await http.get(`/backups/${b.id}/download?what=data`); window.open(r.data.url, "_blank"); }
                catch { const r = await http.get(`/backups/${b.id}/download?what=manifest`).catch(() => null); if (r) window.open(r.data.url, "_blank"); else toast("no download", "err"); }
              }}>Download</Button>
              <Button size="sm" variant="danger" onClick={() => del(`backup ${b.id}`, () => submitJob("delete", `/backups/${b.id}`, null, "delete backup", refetch))}>Del</Button>
            </>
          )}
        />
      </GlassSection>

      <FormModal open={create} onClose={() => setCreate(false)} title="Back up a volume" submitLabel="Back up"
        fields={[
          {
            name: "volume_id", label: "Volume",
            // Backups require a PVC-backed (CSI) volume server-side — filter out raw NFS/ZFS
            // volumes with no pvc_name so they can't be picked only to fail on submit.
            options: (vols || []).filter((v) => v.pvc_name).map((v) => ({ value: v.id, label: v.name })),
            hint: "No PVC-backed volumes to back up yet.",
          },
          {
            name: "bucket_id", label: "Bucket",
            options: (buckets || []).filter((b) => b.state === "bound").map((b) => ({ value: b.id, label: b.bucket_name || b.id })),
            hint: "No bound buckets yet — create one on the Buckets page first.",
          },
          { name: "mode", label: "Mode", options: [{ value: "manifest", label: "manifest" }, { value: "data", label: "data (rbd export-diff)" }] },
          { name: "keep", label: "Keep (0=all)", type: "number", value: "0", min: 0 },
          { name: "max_age_secs", label: "Max age secs (0=off)", type: "number", value: "0", min: 0 },
        ]}
        onSubmit={(v) => submitJob("post", "/backup-jobs", { volume_id: v.volume_id, bucket_id: v.bucket_id, mode: v.mode, keep: +v.keep, max_age_secs: +v.max_age_secs }, "backup", refetch)} />

      {restore && (
        <FormModal open onClose={() => setRestore(null)} title={`Restore ${restore.id}`} submitLabel="Restore"
          fields={[{ name: "name", label: "New volume name (optional)", optional: true }, { name: "mode", label: "Mode", options: [{ value: "snapshot", label: "snapshot" }, { value: "data", label: "data" }] }]}
          onSubmit={(v) => submitJob("post", "/restore-jobs", { backup_id: restore.id, name: v.name || undefined, mode: v.mode }, "restore", () => inv("volumes"))} />
      )}
    </div>
  );
}
