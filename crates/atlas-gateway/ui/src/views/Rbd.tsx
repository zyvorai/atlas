// Copyright (c) 2026 ZyvorAI Labs Private Limited. All rights reserved.
import { useState } from "react";
import { Layers, Plus, RefreshCw } from "lucide-react";
import { http, submit, submitJob } from "../api/client";
import { useInvalidate, useRbdImages } from "../api/hooks";
import { Badge, Button, FormModal, GlassSection, PageHeader, SlideOver } from "../ui/kit";
import { confirmThen, del } from "../ui/confirm";
import { Table } from "../ui/Table";
import { gib } from "../lib/format";

export default function Rbd() {
  const [pool, setPool] = useState("rbd-nvme-prod");
  const { data } = useRbdImages(pool);
  const inv = useInvalidate();
  const refetch = () => inv("rbd");
  const [create, setCreate] = useState(false);
  const [modal, setModal] = useState<{ img: string; kind: string } | null>(null);
  const [snapImg, setSnapImg] = useState<string | null>(null);

  return (
    <div>
      <PageHeader
        icon={Layers}
        title="RBD Images"
        subtitle="Raw Ceph RBD images (bypassing CSI) — for machina/libvirt & bare VMs"
        actions={
          <>
            <input className="field w-44" value={pool} onChange={(e) => setPool(e.target.value)} />
            <Button icon={RefreshCw} onClick={() => submit("post", "/rbd-usage/refresh", null, "usage refresh", () => inv("volumes")).catch(() => {})}>Refresh usage</Button>
            <Button variant="primary" icon={Plus} onClick={() => setCreate(true)}>Image</Button>
          </>
        }
      />
      <GlassSection title={<>Images in {pool} <Badge kind="neutral">{data?.images.length || 0}</Badge></>}>
        <Table
          rows={data?.images}
          rowKey={(i) => i}
          cols={[{ h: "Image", f: (i) => i, mono: true }]}
          actions={(img) => (
            <>
              <Button size="sm" onClick={() => setSnapImg(img)}>Snaps</Button>
              <Button size="sm" onClick={() => setModal({ img, kind: "clone" })}>Clone</Button>
              <Button size="sm" onClick={() => setModal({ img, kind: "resize" })}>Resize</Button>
              <Button size="sm" onClick={() => confirmThen({ title: `Flatten ${img}?`, message: "Detaches the clone from its parent (copies all data).", confirmLabel: "Flatten" }, () => submitJob("post", `/rbd-images/${pool}/${img}/flatten`, {}, `flatten ${img}`, refetch))}>Flatten</Button>
              <Button size="sm" variant="danger" onClick={() => del(`image ${img}`, () => submitJob("delete", `/rbd-images/${pool}/${img}`, null, `delete ${img}`, refetch))}>Del</Button>
            </>
          )}
        />
      </GlassSection>

      <FormModal open={create} onClose={() => setCreate(false)} title="Create RBD image" submitLabel="Create"
        fields={[{ name: "name", label: "Name" }, { name: "size_gib", label: "Size (GiB)", type: "number", value: "1" }]}
        onSubmit={(v) => submitJob("post", "/rbd-images", { name: v.name, size_bytes: gib(+v.size_gib), pool }, `create ${v.name}`, refetch)} />

      {modal?.kind === "clone" && (
        <FormModal open onClose={() => setModal(null)} title={`Clone ${modal.img}`} submitLabel="Clone"
          fields={[{ name: "name", label: "Clone name" }, { name: "snap", label: "Snapshot", value: "base" }]}
          onSubmit={(v) => submitJob("post", `/rbd-images/${pool}/${modal.img}/clone`, { name: v.name, snap: v.snap }, "clone", refetch)} />
      )}
      {modal?.kind === "resize" && (
        <FormModal open onClose={() => setModal(null)} title={`Resize ${modal.img}`} submitLabel="Resize"
          fields={[{ name: "size_gib", label: "New size (GiB)", type: "number" }]}
          onSubmit={(v) => submitJob("post", `/rbd-images/${pool}/${modal.img}/resize`, { size_bytes: gib(+v.size_gib) }, "resize", refetch)} />
      )}

      <RbdSnaps pool={pool} img={snapImg} onClose={() => setSnapImg(null)} />
    </div>
  );
}

function RbdSnaps({ pool, img, onClose }: { pool: string; img: string | null; onClose: () => void }) {
  const [snaps, setSnaps] = useState<string[] | null>(null);
  const [name, setName] = useState("");
  const load = () => img && http.get(`/rbd-images/${pool}/${img}/snapshots`).then((r) => setSnaps(r.data.snapshots || [])).catch(() => setSnaps([]));
  if (img && snaps === null) load();
  const close = () => { setSnaps(null); onClose(); };
  if (!img) return null;
  return (
    <SlideOver open={!!img} onClose={close} title={<span className="mono">{img} · snapshots</span>}>
      <div className="flex gap-2 mb-4">
        <input className="field" placeholder="snapshot name" value={name} onChange={(e) => setName(e.target.value)} />
        <Button variant="primary" disabled={!name} onClick={async () => { await submitJob("post", `/rbd-images/${pool}/${img}/snapshots`, { name }, "snapshot"); setName(""); setTimeout(load, 1200); }}>Create</Button>
      </div>
      <Table
        rows={snaps || []}
        rowKey={(s) => s}
        cols={[{ h: "Snapshot", f: (s) => s, mono: true }]}
        actions={(s) => (
          <Button size="sm" variant="danger" onClick={() => confirmThen({ title: `Roll back to ${s}?`, message: "Destructive: reverts the image to this snapshot's contents.", confirmLabel: "Roll back", danger: true }, () => submitJob("post", `/rbd-images/${pool}/${img}/rollback`, { name: s }, `rollback to ${s}`))}>Rollback</Button>
        )}
      />
    </SlideOver>
  );
}
