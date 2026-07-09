// Copyright (c) 2026 ZyvorAI Labs Private Limited. All rights reserved.
import { useParams } from "react-router-dom";
import { Check, Circle, Loader2, Route as RouteIcon } from "lucide-react";
import { submitJob } from "../../api/client";
import { usePlan, useSource, useInvalidate } from "../../api/hooks";
import { Badge, Button, GlassSection, PageHeader } from "../../ui/kit";
import { planStateKind } from "./Plans";

// Pipeline stages in order. `state` values from migration_plans map to how far we've progressed.
const STAGES = ["discover", "assess", "provision", "full-load", "cdc", "validate", "cutover"] as const;
const STAGE_LABEL: Record<string, string> = {
  discover: "Discover", assess: "Assess", provision: "Provision edge",
  "full-load": "Full-load", cdc: "CDC replication", validate: "Validate", cutover: "Cutover",
};
// migration_plans.state -> index of the last COMPLETED stage.
const COMPLETED_THROUGH: Record<string, number> = {
  draft: -1, discovered: 0, assessed: 1, provisioning: 1, provisioned: 2,
  full_loading: 2, loaded: 3, cdc_streaming: 4, validating: 4, validated: 5,
  cutover_pending: 5, cutover_in_progress: 5, cutover_complete: 6, completed: 6,
  rolled_back: 6, failed: -1,
};
// Which stages are wired so far (later slices flip these on).
const IMPLEMENTED = new Set(["discover", "assess", "provision"]);

export default function PlanDetail() {
  const { id = "" } = useParams();
  const { data: plan } = usePlan(id);
  const { data: source } = useSource(plan?.source_id || "");
  const inv = useInvalidate();
  const refresh = () => inv("db-plan", "db-plans", "db-source", "db-sources");

  if (!plan) return <div className="p-6 text-muted-foreground">Loading plan…</div>;
  const done = COMPLETED_THROUGH[plan.state] ?? -1;
  const current = done + 1; // the next actionable stage index
  const a = plan.assessment as any;

  const act = (stage: string) => {
    if (stage === "discover" && source) return submitJob("post", `/databridge/sources/${source.id}/discover`, null, "discover source", refresh);
    if (stage === "assess") return submitJob("post", `/databridge/plans/${plan.id}/assess`, null, "assess plan", refresh);
    if (stage === "provision") return submitJob("post", `/databridge/plans/${plan.id}/provision`, null, "provision edge", refresh);
  };

  return (
    <div>
      <PageHeader icon={RouteIcon} title={plan.name}
        subtitle={`Plan ${plan.id} · source ${source?.name || plan.source_id}`}
        actions={<Badge kind={planStateKind(plan.state)} dot>{plan.state}</Badge>} />

      <GlassSection title="Pipeline">
        <ol className="flex flex-col gap-2">
          {STAGES.map((stage, i) => {
            const status = i <= done ? "done" : i === current ? "current" : "upcoming";
            const Icon = status === "done" ? Check : status === "current" ? Loader2 : Circle;
            const actionable = status === "current" && IMPLEMENTED.has(stage);
            return (
              <li key={stage} className="flex items-center gap-3 py-1">
                <span className={
                  status === "done" ? "text-emerald-500" : status === "current" ? "text-sky-500" : "text-muted-foreground/40"
                }><Icon size={18} /></span>
                <span className="w-8 text-xs text-muted-foreground">{i + 1}</span>
                <span className={"flex-1 " + (status === "upcoming" ? "text-muted-foreground/60" : "")}>
                  {STAGE_LABEL[stage]}
                  {!IMPLEMENTED.has(stage) && <span className="ml-2 text-xs text-muted-foreground/50">(coming soon)</span>}
                </span>
                {actionable && <Button size="sm" variant="primary" onClick={() => act(stage)}>Run</Button>}
                {status === "done" && <Badge kind="success">done</Badge>}
              </li>
            );
          })}
        </ol>
      </GlassSection>

      {a && typeof a === "object" && "score" in a && (
        <GlassSection title="Assessment">
          <div className="flex gap-6 items-center mb-3">
            <div className="text-3xl font-semibold">{a.score}<span className="text-base text-muted-foreground">/100</span></div>
            <Badge kind={a.risk === "low" ? "success" : a.risk === "medium" ? "warning" : "danger"}>{a.risk} risk</Badge>
            <div className="text-sm text-muted-foreground">Est. downtime: <b>{a.downtime_estimate}</b> · {a.table_count} tables</div>
          </div>
          {!!a.blockers?.length && (
            <div className="mb-2">
              <div className="text-sm font-medium text-red-500 mb-1">Blockers</div>
              <ul className="list-disc ml-5 text-sm">{a.blockers.map((b: string, k: number) => <li key={k}>{b}</li>)}</ul>
            </div>
          )}
          {!!a.warnings?.length && (
            <div>
              <div className="text-sm font-medium text-amber-500 mb-1">Warnings</div>
              <ul className="list-disc ml-5 text-sm">{a.warnings.map((w: string, k: number) => <li key={k}>{w}</li>)}</ul>
            </div>
          )}
          {!a.blockers?.length && !a.warnings?.length && <div className="text-sm text-emerald-500">No blockers or warnings — ready to provision.</div>}
        </GlassSection>
      )}
    </div>
  );
}
