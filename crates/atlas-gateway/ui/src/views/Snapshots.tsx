// Copyright (c) 2026 ZyvorAI Labs Private Limited. All rights reserved.
import { useState } from "react";
import { Camera } from "lucide-react";
import { submitJob } from "../api/client";
import { useInvalidate, useSnapshots } from "../api/hooks";
import type { StorageSnapshot } from "../api/types";
import { Badge, Button, FormModal, GlassSection, PageHeader } from "../ui/kit";
import { del } from "../ui/confirm";
import { Table } from "../ui/Table";
import { stateKind, timeAgo } from "../lib/format";

export default function Snapshots() {
  const { data } = useSnapshots();
  const inv = useInvalidate();
  const refetch = () => inv("snapshots", "volumes");
  const [modal, setModal] = useState<{ s: StorageSnapshot; kind: string } | null>(null);
  return (
    <div>
      <PageHeader icon={Camera} title="Snapshots" subtitle="Point-in-time VolumeSnapshots — clone or restore into new volumes" />
      <GlassSection title={<>Snapshots <Badge kind="neutral">{data?.length || 0}</Badge></>}>
        <Table
          rows={data}
          rowKey={(s) => s.id}
          cols={[
            { h: "Name", f: (s) => s.name, mono: true, sortKey: (s) => s.name },
            { h: "Volume", f: (s) => s.volume_id, mono: true, sortKey: (s) => s.volume_id },
            { h: "State", f: (s) => <Badge kind={stateKind(s.state)} dot>{s.state}</Badge>, sortKey: (s) => s.state },
            { h: "Protected", f: (s) => (s.protected ? "yes" : "no") },
            { h: "Created", f: (s) => <span className="text-muted-foreground">{timeAgo(s.created_at)}</span>, sortKey: (s) => s.created_at || "" },
          ]}
          actions={(s) => (
            <>
              <Button size="sm" onClick={() => setModal({ s, kind: "clone" })}>Clone</Button>
              <Button size="sm" onClick={() => setModal({ s, kind: "restore" })}>Restore</Button>
              <Button size="sm" variant="danger" onClick={() => del(`snapshot ${s.name}`, () => submitJob("delete", `/snapshots/${s.id}?force=true`, null, "delete snapshot", refetch))}>Del</Button>
            </>
          )}
        />
      </GlassSection>
      {modal && (
        <FormModal open onClose={() => setModal(null)} title={`${modal.kind === "clone" ? "Clone" : "Restore"} ${modal.s.name}`}
          submitLabel={modal.kind === "clone" ? "Clone" : "Restore"}
          fields={[{ name: "name", label: "New volume name" }, { name: "namespace", label: "Namespace", value: "rook-ceph" }]}
          onSubmit={(v) => submitJob("post", `/snapshots/${modal.s.id}/${modal.kind}`, { name: v.name, namespace: v.namespace }, modal.kind, refetch)} />
      )}
    </div>
  );
}
