<!-- Copyright (c) 2026 ZyvorAI Labs Private Limited. -->
<!-- SPDX-License-Identifier: AGPL-3.0-only OR LicenseRef-Atlas-Commercial -->
# Atlas Ops Advisor

Atlas Ops Advisor turns current Atlas telemetry into an explainable risk score and a prioritized,
read-only runbook. The endpoint never executes an action and does not require a model.

Operators can use **Storage Center → Observability → Ops Advisor** to ask a focused question,
choose local/automatic/LLM analysis, inspect evidence, and follow the prioritized runbook.

```bash
curl -sS -X POST http://127.0.0.1:5110/api/atlas/v1/ai/advisor \
  -H 'Content-Type: application/json' \
  -d '{"question":"What should the storage team handle first?","mode":"local"}'
```

The response contains:

- a bounded `risk_score` and `risk_level`;
- evidence from capacity, the 14-day forecast, recovery counters, open alerts, and recent failures;
- prioritized actions pointing only to read-only Atlas endpoints;
- `can_execute: false`, an explicit automation safety boundary.

## Modes

| Mode | Behavior |
| --- | --- |
| `local` | Deterministic analysis; no operational context leaves Atlas. |
| `auto` | Uses an OpenAI-compatible provider when configured, otherwise local; safely falls back on provider failure. |
| `llm` | Requires a configured provider and returns `503` if it fails. |

Configure an optional provider with `ATLAS_AI_BASE_URL`, `ATLAS_AI_MODEL`, and optionally
`ATLAS_AI_API_KEY` and `ATLAS_AI_TIMEOUT_SECS`. The provider may rewrite only the executive summary;
Atlas computes and retains authority over the score, evidence, and actions. Remote endpoints must
use HTTPS. Plain HTTP is permitted only for a loopback model endpoint.

When authentication is enabled, the endpoint requires an operator-or-higher token and records an
`ai.advisor` audit event. Questions are capped at 512 characters, model output at 2,000 characters,
and only aggregate telemetry plus at most 20 alert titles is sent to the provider.
