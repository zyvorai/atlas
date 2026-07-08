// Copyright (c) 2026 ZyvorAI Labs Private Limited. All rights reserved.
import { useState } from "react";
import { Activity as ActivityIcon, Camera, ClipboardList, Clock } from "lucide-react";
import { useEvents } from "../api/hooks";
import type { ActivityEvent } from "../api/types";
import { Badge, Button, GlassSection, PageHeader } from "../ui/kit";
import { stateKind, timeAgo } from "../lib/format";

const KIND_ICON = { job: Clock, audit: ClipboardList, alert: Camera } as const;

export default function Activity() {
  const [kind, setKind] = useState<"" | ActivityEvent["kind"]>("");
  const { data } = useEvents(150);
  const rows = (data || []).filter((e) => !kind || e.kind === kind);
  const filters: Array<{ k: "" | ActivityEvent["kind"]; label: string }> = [
    { k: "", label: "All" },
    { k: "job", label: "Jobs" },
    { k: "audit", label: "Audit" },
    { k: "alert", label: "Alerts" },
  ];

  return (
    <div>
      <PageHeader icon={ActivityIcon} title="Activity" subtitle="Unified timeline of jobs, audited actions, and alerts" />
      <div className="flex gap-2 mb-3">
        {filters.map((f) => (
          <Button key={f.k || "all"} size="sm" variant={kind === f.k ? "primary" : "secondary"} onClick={() => setKind(f.k)}>{f.label}</Button>
        ))}
      </div>
      <GlassSection title={<>Timeline <Badge kind="neutral">{rows.length}</Badge></>}>
        {!data ? (
          <div className="p-6 text-sm text-muted-foreground">Loading…</div>
        ) : rows.length === 0 ? (
          <div className="p-6 text-sm text-muted-foreground">No activity.</div>
        ) : (
          <ul className="divide-y divide-white/5">
            {rows.map((e) => {
              const Icon = KIND_ICON[e.kind] || ActivityIcon;
              return (
                <li key={`${e.kind}-${e.id}-${e.ts}`} className="flex items-start gap-3 px-3 py-2.5">
                  <Icon size={16} className="mt-0.5 text-muted-foreground shrink-0" />
                  <div className="min-w-0 flex-1">
                    <div className="flex items-center gap-2 flex-wrap">
                      <Badge kind={stateKind(e.severity)} dot>{e.severity}</Badge>
                      <span className="font-medium">{e.title}</span>
                      <span className="text-xs text-muted-foreground mono">{e.resource_type}/{e.resource_id}</span>
                    </div>
                    <div className="text-xs text-muted-foreground mt-0.5 truncate">{e.detail}</div>
                  </div>
                  <div className="text-right shrink-0">
                    <div className="text-xs text-muted-foreground">{timeAgo(e.ts)}</div>
                    <div className="text-[10px] text-muted-foreground/70 mono">{e.kind} · {e.actor}</div>
                  </div>
                </li>
              );
            })}
          </ul>
        )}
      </GlassSection>
    </div>
  );
}
