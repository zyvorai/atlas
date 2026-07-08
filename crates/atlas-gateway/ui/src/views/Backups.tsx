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

  return (
    <div>
      <PageHeader icon={Archive} title="Backups" subtitle="RBD backups to RGW (manifest + streamed data), restore & presigned download"
        actions={<Button variant="primary" icon={Plus} onClick={() => setCreate(true)}>Backup</Button>} />
      <GlassSection title={<>Backups <Badge kind="neutral">{data?.length || 0}</Badge></>}>
        <Table
          rows={data}
          rowKey={(b) => b.id}
          empty="No backups yet."
          emptyCta={<Button variant="primary" icon={Plus} onClick={() => setCreate(true)}>Back up a volume</Button>}
          cols={[
            { h: "ID", f: (b) => b.id, mono: true },
            { h: "Volume", f: (b) => b.volume_id, mono: true },
            { h: "Format", f: (b) => b.format },
            { h: "State", f: (b) => <Badge kind={stateKind(b.state)} dot>{b.state}</Badge> },
            { h: "Checksum", f: (b) => <span className="mono text-muted-foreground">{(b.checksum || "").slice(0, 12) || "—"}</span> },
            { h: "Created", f: (b) => <span className="text-muted-foreground">{timeAgo(b.created_at)}</span> },
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
          { name: "volume_id", label: "Volume", options: (vols || []).map((v) => ({ value: v.id, label: v.name })) },
          { name: "bucket_id", label: "Bucket", options: (buckets || []).filter((b) => b.state === "bound").map((b) => ({ value: b.id, label: b.bucket_name || b.id })) },
          { name: "mode", label: "Mode", options: [{ value: "manifest", label: "manifest" }, { value: "data", label: "data (rbd export-diff)" }] },
          { name: "keep", label: "Keep (0=all)", type: "number", value: "0" },
          { name: "max_age_secs", label: "Max age secs (0=off)", type: "number", value: "0" },
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
