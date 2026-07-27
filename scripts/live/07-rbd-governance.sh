#!/usr/bin/env bash
# Copyright (c) 2026 ZyvorAI Labs Private Limited. All rights reserved.
# RBD listing, usage refresh, maintenance, upgrade preflight.
set -euo pipefail
# shellcheck source=lib.sh
source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/lib.sh"

section "07-rbd-governance"

assert_ok GET /rbd-images || true
assert_ok POST /rbd-usage/refresh '{}' || true
assert_ok GET /maintenance || true
assert_ok GET /maintenance/orphans || true
assert_ok GET /upgrade/preflight || true

live_req GET /upgrade/preflight
if [[ "$LIVE_CODE" =~ ^2 ]]; then
  READY="$(python3 -c '
import sys, json
d = json.loads(sys.stdin.read() or "{}")
# ready may be top-level or inferred from empty blockers
if "ready" in d:
    print("true" if d["ready"] else "false")
else:
    blockers = d.get("blockers") or []
    print("true" if len(blockers) == 0 else "false")
' <<<"$LIVE_BODY")"
  info "upgrade preflight ready=${READY}"
fi
