// Copyright (c) 2026 ZyvorAI Labs Private Limited. All rights reserved.
import { useEffect, useRef, useState } from "react";
import { Cloud, Download, Plus, Upload } from "lucide-react";
import { apiError, http, submitJob, toast } from "../api/client";
import { useBuckets, useInvalidate } from "../api/hooks";
import type { StorageBucket } from "../api/types";
import { Badge, Button, FormModal, GlassSection, PageHeader, SlideOver } from "../ui/kit";
import { del } from "../ui/confirm";
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
      <PageHeader icon={Cloud} title="Buckets" subtitle="RGW object buckets (ObjectBucketClaim) — quotas, stats, and browse / upload / download / delete objects"
        actions={<Button variant="primary" icon={Plus} onClick={() => setCreate(true)}>Bucket</Button>} />
      <GlassSection title={<>Buckets <Badge kind="neutral">{data?.length || 0}</Badge></>}>
        <Table
          rows={data}
          rowKey={(b) => b.id}
          empty="No buckets yet."
          emptyCta={<Button variant="primary" icon={Plus} onClick={() => setCreate(true)}>Create bucket</Button>}
          cols={[
            { h: "Name", f: (b) => b.bucket_name || b.name || b.id, mono: true },
            { h: "State", f: (b) => <Badge kind={b.state === "bound" ? "success" : "warning"} dot>{b.state}</Badge> },
            { h: "Namespace", f: (b) => b.namespace },
            { h: "Endpoint", f: (b) => <span className="mono text-muted-foreground">{b.endpoint || "—"}</span> },
          ]}
          actions={(b) => (
            <>
              <Button size="sm" onClick={async () => {
                try { const s = await http.get(`/buckets/${b.id}/stats`); const d = s.data;
                  toast(`${d.bucket}: ${num(d.num_objects)} objs, ${fmtBytes(d.size_bytes)}`, "ok");
                } catch (e) { toast(`stats: ${apiError(e)}`, "err"); }
              }}>Stats</Button>
              <Button size="sm" onClick={() => setObjBucket(b)}>Objects</Button>
              <Button size="sm" variant="danger" onClick={() => del(`bucket ${b.bucket_name || b.name || b.id}`, () => submitJob("delete", `/buckets/${b.id}?force=true`, null, "delete bucket", refetch))}>Del</Button>
            </>
          )}
        />
      </GlassSection>

      <FormModal open={create} onClose={() => setCreate(false)} title="Create bucket" submitLabel="Create"
        fields={[
          {
            name: "name", label: "Name",
            pattern: /^[a-z0-9][a-z0-9.-]{1,61}[a-z0-9]$/,
            hint: "3-63 chars: lowercase letters, digits, dots, hyphens (S3/RGW bucket naming rules).",
          },
          { name: "namespace", label: "Namespace", value: "rook-ceph" },
          { name: "max_objects", label: "Max objects (optional)", type: "number", optional: true, min: 0 },
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
  const [listError, setListError] = useState(false);
  const [prefix, setPrefix] = useState("");
  const [busy, setBusy] = useState(false);
  const [keep, setKeep] = useState(0); // 0 = overwrite in place; >0 = keep N timestamped versions
  const fileRef = useRef<HTMLInputElement>(null);
  const load = (p = "") => bucket && http.get(`/buckets/${bucket.id}/objects${p ? "?prefix=" + encodeURIComponent(p) : ""}`)
    .then((r) => { setObjs(r.data.objects || []); setListError(false); })
    .catch((e) => { setObjs([]); setListError(true); toast(`list objects: ${apiError(e)}`, "err"); });
  useEffect(() => { if (bucket) load(); }, [bucket]);
  const close = () => { setObjs(null); setListError(false); setPrefix(""); onClose(); };
  if (!bucket) return null;

  // Upload straight to RGW: gateway mints a presigned PUT, the browser PUTs the file to it —
  // the object bytes never pass through atlas-gateway, so large db files scale fine. With keep>0
  // each upload is stored as a timestamped version and old ones are pruned to keep N (db backups).
  const upload = async (files: FileList | null) => {
    if (!files || !files.length) return;
    setBusy(true);
    try {
      for (const file of Array.from(files)) {
        const key = (prefix ? prefix.replace(/\/+$/, "") + "/" : "") + file.name;
        const versioned = keep > 0;
        const r = await http.post(`/buckets/${bucket.id}/objects/upload-url`, { key, versioned });
        const put = await fetch(r.data.url, { method: "PUT", body: file });
        if (!put.ok) throw new Error(`upload ${file.name}: HTTP ${put.status}`);
        if (versioned) await http.post(`/buckets/${bucket.id}/objects/prune`, { prefix: r.data.base_key + ".", keep });
      }
      toast(`uploaded ${files.length} file(s)${keep > 0 ? `, keeping ${keep} versions` : ""}`, "ok");
      await load(prefix);
    } catch (e) { toast(String(e), "err"); }
    finally { setBusy(false); }
  };

  const download = async (key: string) => {
    try {
      const r = await http.get(`/buckets/${bucket.id}/objects/download-url?key=${encodeURIComponent(key)}`);
      window.open(r.data.url, "_blank");
    } catch (e) { toast(String(e), "err"); }
  };

  const remove = (key: string) => del(`object ${key}`, async () => {
    try { await http.delete(`/buckets/${bucket.id}/objects?key=${encodeURIComponent(key)}`); toast("deleted", "ok"); await load(prefix); }
    catch (e) { toast(String(e), "err"); }
  });

  return (
    <SlideOver open={!!bucket} onClose={close} title={<span className="mono">{bucket.bucket_name || bucket.name || bucket.id} · objects</span>} width={560}>
      <div className="flex gap-2 mb-3 items-center">
        <input className="field" placeholder="prefix / folder…" value={prefix} onChange={(e) => setPrefix(e.target.value)} />
        <Button onClick={() => load(prefix)}>List</Button>
        <input className="field" style={{ width: 84 }} type="number" min={0} title="Keep N timestamped versions per file (0 = overwrite in place)" placeholder="keep" value={keep} onChange={(e) => setKeep(Math.max(0, +e.target.value || 0))} />
        <Button variant="primary" icon={Upload} loading={busy} onClick={() => fileRef.current?.click()}>Upload</Button>
        <input ref={fileRef} type="file" multiple hidden onChange={(e) => { upload(e.target.files); e.currentTarget.value = ""; }} />
      </div>
      <Table rows={objs ?? undefined} rowKey={(o) => o.key}
        cols={[{ h: "Key", f: (o) => o.key, mono: true }, { h: "Size", f: (o) => fmtBytes(o.size_bytes) }]}
        actions={(o) => (
          <>
            <Button size="sm" icon={Download} onClick={() => download(o.key)}>Get</Button>
            <Button size="sm" variant="danger" onClick={() => remove(o.key)}>Del</Button>
          </>
        )}
        empty={listError ? "Couldn't list objects (see toast for the error)." : "No objects — upload one above."} />
    </SlideOver>
  );
}
