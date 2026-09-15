// Copyright (c) 2026 ZyvorAI Labs Private Limited.
// SPDX-License-Identifier: AGPL-3.0-only OR LicenseRef-Atlas-Commercial
// Ops Advisor — explainable AI posture and read-only runbooks. No action execution by design.
import { useState, type FormEvent } from "react";
import {
  AlertTriangle,
  BrainCircuit,
  CheckCircle2,
  Eye,
  LockKeyhole,
  RefreshCw,
  ShieldCheck,
  Sparkles,
} from "lucide-react";
import { apiError, http } from "../api/client";
import type { AdvisorMode, AdvisorResponse } from "../api/types";
import { navCrumbs } from "../nav/routes";
import { Badge, Button, RadialGauge } from "../ui/kit";
import { DashboardHero } from "../ui/templates/DashboardHero";

const PROMPTS = [
  "What should the storage team handle first?",
  "Explain the current capacity risk.",
  "Are recovery signals affecting data safety?",
];

function riskKind(level: AdvisorResponse["risk_level"]) {
  if (level === "critical") return "danger" as const;
  if (level === "high" || level === "moderate") return "warning" as const;
  return "success" as const;
}

function modeLabel(mode: AdvisorResponse["mode"]) {
  if (mode === "llm") return "Model enhanced";
  if (mode === "local_fallback") return "Local fallback";
  return "Local analysis";
}

export default function OpsAdvisor() {
  const [question, setQuestion] = useState(PROMPTS[0]);
  const [mode, setMode] = useState<AdvisorMode>("local");
  const [result, setResult] = useState<AdvisorResponse>();
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState("");

  async function analyze(e?: FormEvent) {
    e?.preventDefault();
    setLoading(true);
    setError("");
    try {
      const { data } = await http.post<AdvisorResponse>("/ai/advisor", { question, mode });
      setResult(data);
    } catch (err) {
      setError(apiError(err));
    } finally {
      setLoading(false);
    }
  }

  return (
    <DashboardHero
      className="at-stack"
      crumbs={navCrumbs("ops-advisor")}
      eyebrow="OBSERVABILITY · EXPLAINABLE AI"
      title="Ops Advisor"
      state={
        result
          ? `${result.risk_level} risk · ${result.actions.length} prioritized step${result.actions.length === 1 ? "" : "s"} · advisory only`
          : "Ask Atlas to correlate capacity, recovery, alerts, forecasts, and recent job failures."
      }
      actions={
        result ? (
          <Button icon={RefreshCw} loading={loading} onClick={() => analyze()}>
            Refresh
          </Button>
        ) : undefined
      }
    >
      <form className="at-panel" onSubmit={analyze}>
        <div className="at-panel-bar">
          <BrainCircuit size={16} aria-hidden />
          <span className="at-caption">Ask about the live storage posture</span>
          <span className="grow" />
          <Badge kind="info">No autonomous actions</Badge>
        </div>
        <div style={{ padding: 18, display: "grid", gap: 14 }}>
          <textarea
            className="field"
            rows={3}
            maxLength={512}
            value={question}
            aria-label="Advisor question"
            placeholder="What should the storage team handle first?"
            onChange={(e) => setQuestion(e.target.value)}
            style={{ width: "100%", resize: "vertical" }}
          />
          <div className="at-chips" style={{ padding: 0, margin: 0 }}>
            {PROMPTS.map((prompt) => (
              <button
                key={prompt}
                className={`at-chip${question === prompt ? " on" : ""}`}
                type="button"
                onClick={() => setQuestion(prompt)}
              >
                {prompt}
              </button>
            ))}
          </div>
          <div style={{ display: "flex", alignItems: "center", gap: 10, flexWrap: "wrap" }}>
            <div className="at-chips" style={{ padding: 0, margin: 0 }} aria-label="Analysis mode">
              {(["local", "auto", "llm"] as AdvisorMode[]).map((value) => (
                <button
                  key={value}
                  className={`at-chip${mode === value ? " on" : ""}`}
                  type="button"
                  onClick={() => setMode(value)}
                >
                  {value === "llm" ? "LLM" : value[0].toUpperCase() + value.slice(1)}
                </button>
              ))}
            </div>
            <span className="at-sub" style={{ margin: 0, flex: 1 }}>
              {mode === "local"
                ? "Telemetry stays inside Atlas."
                : "Aggregate telemetry may be sent to the configured model endpoint."}
            </span>
            <Button type="submit" variant="primary" icon={Sparkles} loading={loading} disabled={!question.trim()}>
              Analyze posture
            </Button>
          </div>
        </div>
      </form>

      {error ? (
        <div className="at-panel" role="alert">
          <div className="at-list-row" style={{ color: "var(--at-fail)" }}>
            <AlertTriangle size={17} aria-hidden />
            <span>{error}</span>
          </div>
        </div>
      ) : null}

      {!result && !error ? (
        <div className="at-panel">
          <div className="at-list-row">
            <ShieldCheck size={18} style={{ color: "var(--at-ok)" }} aria-hidden />
            <div>
              <div className="at-val" style={{ fontSize: 15 }}>Atlas remains the authority</div>
              <p className="at-sub" style={{ margin: "4px 0 0" }}>
                The model can summarize evidence, but it cannot change the risk score, rewrite the runbook, or execute storage operations.
              </p>
            </div>
          </div>
        </div>
      ) : null}

      {result ? (
        <>
          <div className="at-panel">
            <div className="at-panel-bar">
              <span className="at-caption">Current assessment</span>
              <Badge kind={riskKind(result.risk_level)} dot>{result.risk_level}</Badge>
              <Badge kind="neutral">{modeLabel(result.mode)}</Badge>
              <span className="grow" />
              <Badge kind="success"><LockKeyhole size={12} /> advisory only</Badge>
            </div>
            <div style={{ padding: 20, display: "flex", alignItems: "center", gap: 24, flexWrap: "wrap" }}>
              <RadialGauge pct={result.risk_score} size={132} label="RISK SCORE" />
              <div style={{ flex: "1 1 360px" }}>
                <div className="at-val md" style={{ textTransform: "capitalize" }}>{result.risk_level} risk</div>
                <p className="at-sub" style={{ margin: "8px 0 0", maxWidth: 760 }}>{result.summary}</p>
                {result.warnings.map((warning) => (
                  <div key={warning} style={{ color: "var(--at-warn)", marginTop: 10, fontSize: 13 }}>
                    <AlertTriangle size={14} style={{ display: "inline", marginRight: 6 }} />{warning}
                  </div>
                ))}
              </div>
            </div>
          </div>

          <div className="at-instrs">
            <div className="at-instr">
              <div className="at-caption">Capacity used</div>
              <div className="at-val md">{result.evidence.capacity_used_percent.toFixed(1)}%</div>
              <div className="at-delta">live inventory</div>
            </div>
            <div className="at-instr">
              <div className="at-caption">Days to full</div>
              <div className="at-val md">{result.evidence.days_to_full == null ? "—" : result.evidence.days_to_full.toFixed(1)}</div>
              <div className="at-delta">14-day regression</div>
            </div>
            <div className="at-instr">
              <div className="at-caption">Open alerts</div>
              <div className="at-val md">{result.evidence.open_alerts}</div>
              <div className="at-delta mono">{result.evidence.critical_alerts} critical · {result.evidence.warning_alerts} warning</div>
            </div>
            <div className="at-instr">
              <div className="at-caption">Recent failures</div>
              <div className="at-val md">{result.evidence.failed_jobs_15m}</div>
              <div className="at-delta">last 15 minutes</div>
            </div>
          </div>

          <div className="at-panel">
            <div className="at-panel-bar">
              <span className="at-caption">Prioritized runbook</span>
              <span className="grow" />
              <Badge kind="neutral">read-only inspection</Badge>
            </div>
            {result.actions.map((action, index) => (
              <div className="at-list-row" key={`${action.priority}-${action.title}`} style={{ alignItems: "flex-start" }}>
                <Badge kind={action.priority === 1 ? "danger" : action.priority <= 3 ? "warning" : "neutral"}>
                  P{action.priority}
                </Badge>
                <div style={{ flex: 1 }}>
                  <div style={{ fontWeight: 650 }}>{index + 1}. {action.title}</div>
                  <p className="at-sub" style={{ margin: "4px 0 8px" }}>{action.rationale}</p>
                  <code className="mono" style={{ fontSize: 11.5 }}>{action.inspect}</code>
                </div>
                <Eye size={16} style={{ color: "var(--at-ink-4)", marginTop: 2 }} aria-label="Inspect only" />
              </div>
            ))}
          </div>

          <div className="at-panel">
            <div className="at-panel-bar"><span className="at-caption">Safety boundary</span></div>
            <div className="at-list-row" style={{ color: "var(--at-ok)" }}>
              <CheckCircle2 size={17} aria-hidden />
              <span>Advisor response confirms <span className="mono">can_execute: {String(result.can_execute)}</span>. Every proposed step points to a read-only inspection endpoint.</span>
            </div>
          </div>
        </>
      ) : null}
    </DashboardHero>
  );
}
