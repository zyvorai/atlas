// Copyright (c) 2026 ZyvorAI Labs Private Limited. All rights reserved.
import { useState } from "react";
import { Copy, KeyRound } from "lucide-react";
import { apiError, http, toast } from "../api/client";
import { Button, Field, GlassSection, Label, PageHeader, Select } from "../ui/kit";
import { Badge } from "../ui/kit";

export default function Access() {
  const [subject, setSubject] = useState("veyron");
  const [role, setRole] = useState("operator");
  const [ttl, setTtl] = useState("3600");
  const [result, setResult] = useState<any>(null);
  const [busy, setBusy] = useState(false);
  return (
    <div>
      <PageHeader icon={KeyRound} title="Access" subtitle="Mint scoped service-account JWTs for products (admin). The secret never leaves Atlas." />
      <div className="grid lg:grid-cols-2 gap-4">
        <GlassSection title="Issue token">
          <div className="p-4">
            <Label>Subject (service account)</Label>
            <Field value={subject} onChange={(e) => setSubject(e.target.value)} placeholder="veyron" />
            <Label>Role</Label>
            <Select value={role} onChange={(e) => setRole(e.target.value)}>
              {["viewer", "operator", "admin"].map((r) => <option key={r}>{r}</option>)}
            </Select>
            <Label>TTL seconds (clamped 60…7776000)</Label>
            <Field type="number" value={ttl} onChange={(e) => setTtl(e.target.value)} />
            <div className="mt-4">
              <Button variant="primary" loading={busy} onClick={async () => {
                setBusy(true);
                try { const r = await http.post("/auth/tokens", { subject, role, ttl_secs: +ttl }); setResult(r.data); toast("token minted", "ok"); }
                catch (e) { toast(apiError(e), "err"); } finally { setBusy(false); }
              }}>Mint token</Button>
            </div>
          </div>
        </GlassSection>
        <GlassSection title="Result">
          <div className="p-4 space-y-3 text-sm">
            {result ? (
              <>
                <div className="flex gap-2 flex-wrap">
                  <Badge kind="info">subject: {result.subject}</Badge>
                  <Badge kind="info">role: {result.role}</Badge>
                  <Badge kind="neutral">level: {result.level}</Badge>
                  <Badge kind="neutral">exp: {result.expires_at}</Badge>
                </div>
                <div className="section-label">Token</div>
                <div className="glass-card p-2 mono text-xs break-all flex items-start gap-2">
                  <span className="flex-1">{result.token}</span>
                  <button className="btn btn-ghost btn-sm" onClick={() => { navigator.clipboard.writeText(result.token); toast("copied", "ok"); }}><Copy size={13} /></button>
                </div>
                <div className="text-muted-foreground text-xs">Send as <code className="mono">Authorization: Bearer &lt;token&gt;</code>, or paste it into the menu-bar key icon to use this console as that identity.</div>
              </>
            ) : <div className="text-muted-foreground">Mint a token to see it here.</div>}
          </div>
        </GlassSection>
      </div>
    </div>
  );
}
