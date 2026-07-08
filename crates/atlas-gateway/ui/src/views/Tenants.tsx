// Copyright (c) 2026 ZyvorAI Labs Private Limited. All rights reserved.
import { useState } from "react";
import { Users } from "lucide-react";
import { http, submit } from "../api/client";
import { useInvalidate, useTenantPolicies, useTenants } from "../api/hooks";
import type { TenantQuota } from "../api/types";
import { Badge, Button, FormModal, GlassSection, PageHeader, SlideOver } from "../ui/kit";
import { del } from "../ui/confirm";
import { Table } from "../ui/Table";
import { fmtBytes, num } from "../lib/format";

export default function Tenants() {
  const { data } = useTenants();
  const inv = useInvalidate();
  const [quota, setQuota] = useState<TenantQuota | null>(null);
  const [drill, setDrill] = useState<TenantQuota | null>(null);
  return (
    <div>
      <PageHeader icon={Users} title="Tenants" subtitle="Per-tenant capacity quotas and intent→placement policy overrides" />
      <GlassSection title={<>Tenants <Badge kind="neutral">{data?.length || 0}</Badge></>}>
        <Table
          rows={data}
          rowKey={(t) => t.tenant_id}
          cols={[
            { h: "Tenant", f: (t) => t.tenant_id, mono: true },
            { h: "Used", f: (t) => fmtBytes(t.used_bytes) },
            { h: "Volumes", f: (t) => num(t.volume_count) },
            { h: "Max bytes", f: (t) => (t.max_bytes ? fmtBytes(t.max_bytes) : "∞") },
            { h: "Max vols", f: (t) => t.max_volumes || "∞" },
          ]}
          actions={(t) => (
            <>
              <Button size="sm" onClick={() => setQuota(t)}>Quota</Button>
              <Button size="sm" onClick={() => setDrill(t)}>Policies</Button>
            </>
          )}
        />
      </GlassSection>

      {quota && (
        <FormModal open onClose={() => setQuota(null)} title={`Quota — ${quota.tenant_id}`} submitLabel="Save"
          fields={[
            { name: "max_bytes", label: "Max bytes (0=∞)", type: "number", value: String(quota.max_bytes) },
            { name: "max_volumes", label: "Max volumes (0=∞)", type: "number", value: String(quota.max_volumes) },
          ]}
          onSubmit={(v) => submit("put", `/tenants/${quota.tenant_id}/quota`, { max_bytes: +v.max_bytes, max_volumes: +v.max_volumes }, "quota set", () => inv("tenants"))} />
      )}
      <PolicyDrawer tenant={drill} onClose={() => setDrill(null)} />
    </div>
  );
}

function PolicyDrawer({ tenant, onClose }: { tenant: TenantQuota | null; onClose: () => void }) {
  const id = tenant?.tenant_id || "";
  const { data, refetch } = useTenantPolicies(id);
  const [intent, setIntent] = useState("database");
  const [sc, setSc] = useState("zyvor-cephfs-shared");
  const [am, setAm] = useState("ReadWriteMany");
  if (!tenant) return null;
  return (
    <SlideOver open={!!tenant} onClose={onClose} title={`Policies — ${tenant.tenant_id}`} width={520}>
      <Table
        rows={data}
        rowKey={(p) => p.intent}
        cols={[
          { h: "Intent", f: (p) => p.intent },
          { h: "StorageClass", f: (p) => p.storage_class, mono: true },
          { h: "Access", f: (p) => p.access_mode },
        ]}
        actions={(p) => (
          <Button size="sm" variant="danger" onClick={() => del(`override ${p.intent}`, async () => { await submit("delete", `/tenants/${id}/policies/${p.intent}`, null, "delete override"); refetch(); })}>Del</Button>
        )}
        empty="No overrides — falls back to the built-in catalog."
      />
      <div className="glass-card p-3 mt-4 space-y-2">
        <div className="section-label">Add override</div>
        <div className="grid grid-cols-3 gap-2">
          <input className="field" placeholder="intent" value={intent} onChange={(e) => setIntent(e.target.value)} />
          <input className="field" placeholder="storage class" value={sc} onChange={(e) => setSc(e.target.value)} />
          <input className="field" placeholder="access mode" value={am} onChange={(e) => setAm(e.target.value)} />
        </div>
        <Button variant="primary" onClick={async () => { await submit("put", `/tenants/${id}/policies/${intent}`, { storage_class: sc, access_mode: am, volume_mode: "Filesystem" }, "override set"); refetch(); }}>Save override</Button>
      </div>
    </SlideOver>
  );
}
