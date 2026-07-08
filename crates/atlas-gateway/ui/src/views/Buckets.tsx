// Copyright (c) 2026 ZyvorAI Labs Private Limited. All rights reserved.
import { useState } from "react";
import { Cloud, Plus } from "lucide-react";
import { http, submitJob } from "../api/client";
import { useBuckets, useInvalidate } from "../api/hooks";
import type { StorageBucket } from "../api/types";
import { Badge, Button, FormModal, GlassSection, PageHeader, SlideOver } from "../ui/kit";
import { Table } from "../ui/Table";
import { fmtBytes, num } from "../lib/format";

export default function Buckets() {
  const { data } = useBuckets();
  const inv = useInvalidate();
  const refetch = () => inv("buckets", "summary");
  const [create, setCreate] = useState(false);
  const [objBucket, setObjBucket] = useState<StorageBucket | null>(null);

  return (
    <div>
      <PageHeader icon={Cloud} title="Buckets" subtitle="RGW object buckets (ObjectBucketClaim) with quotas, stats & object browser"
        actions={<Button variant="primary" icon={Plus} onClick={() => setCreate(true)}>Bucket</Button>} />
      <GlassSection title={<>Buckets <Badge kind="neutral">{data?.length || 0}</Badge></>}>
        <Table
          rows={data}
          rowKey={(b) => b.id}
          cols={[
            { h: "Name", f: (b) => b.bucket_name || b.id, mono: true },
            { h: "State", f: (b) => <Badge kind={b.state === "bound" ? "success" : "warning"} dot>{b.state}</Badge> },
            { h: "Namespace", f: (b) => b.namespace },
            { h: "Endpoint", f: (b) => <span className="mono text-muted-foreground">{b.endpoint || "—"}</span> },
          ]}
          actions={(b) => (
            <>
              <Button size="sm" onClick={async () => {
                try { const s = await http.get(`/buckets/${b.id}/stats`); const d = s.data;
                  (await import("../api/client")).toast(`${d.bucket}: ${num(d.num_objects)} objs, ${fmtBytes(d.size_bytes)}`, "ok");
                } catch (e) { (await import("../api/client")).toast(String(e), "err"); }
              }}>Stats</Button>
              <Button size="sm" onClick={() => setObjBucket(b)}>Objects</Button>
              <Button size="sm" variant="danger" onClick={() => submitJob("delete", `/buckets/${b.id}?force=true`, null, "delete bucket", refetch)}>Del</Button>
            </>
          )}
        />
      </GlassSection>

      <FormModal open={create} onClose={() => setCreate(false)} title="Create bucket" submitLabel="Create"
        fields={[
          { name: "name", label: "Name" }, { name: "namespace", label: "Namespace", value: "rook-ceph" },
          { name: "max_objects", label: "Max objects (optional)", type: "number", optional: true },
          { name: "max_size", label: "Max size (e.g. 2G, optional)", optional: true },
        ]}
        onSubmit={(v) => {
          const body: Record<string, unknown> = { name: v.name, namespace: v.namespace };
          if (v.max_objects) body.max_objects = +v.max_objects;
          if (v.max_size) body.max_size = v.max_size;
          return submitJob("post", "/buckets", body, "bucket", refetch);
        }} />

      <ObjectBrowser bucket={objBucket} onClose={() => setObjBucket(null)} />
    </div>
  );
}

function ObjectBrowser({ bucket, onClose }: { bucket: StorageBucket | null; onClose: () => void }) {
  const [objs, setObjs] = useState<{ key: string; size_bytes: number }[] | null>(null);
  const [prefix, setPrefix] = useState("");
  const load = (p = "") => bucket && http.get(`/buckets/${bucket.id}/objects${p ? "?prefix=" + p : ""}`).then((r) => setObjs(r.data.objects || [])).catch(() => setObjs([]));
  if (bucket && objs === null) load();
  const close = () => { setObjs(null); onClose(); };
  if (!bucket) return null;
  return (
    <SlideOver open={!!bucket} onClose={close} title={<span className="mono">{bucket.bucket_name || bucket.id} · objects</span>} width={560}>
      <div className="flex gap-2 mb-3">
        <input className="field" placeholder="prefix…" value={prefix} onChange={(e) => setPrefix(e.target.value)} />
        <Button variant="primary" onClick={() => load(prefix)}>List</Button>
      </div>
      <Table rows={objs || []} rowKey={(o) => o.key}
        cols={[{ h: "Key", f: (o) => o.key, mono: true }, { h: "Size", f: (o) => fmtBytes(o.size_bytes) }]} empty="No objects." />
    </SlideOver>
  );
}
