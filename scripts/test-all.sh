#!/usr/bin/env bash
# Copyright (c) 2026 ZyvorAI Labs Private Limited. All rights reserved.
#
# One-shot "test all features" runner. Executes the tiers that need no external infrastructure and
# then, if a container runtime is present, the real-database connector verification.
#
#   Tier 0  static gate (clippy + workspace tests + build) + optional-feature compile checks
#   Tier 1  fake-driver gap tests (folded into the workspace test run)
#   Tier 2  real DB connectors via ephemeral containers (scripts/test-connectors.sh)
#
# Tier 3 (real Ceph/k8s write path on a remote cluster) and Tier 4 (UI smoke, needs npm) are printed
# as opt-in follow-ups — they mutate a remote or need a browser toolchain, so they aren't automatic.
#
# Usage: scripts/test-all.sh [--no-containers]
set -euo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$HERE"

NO_CONTAINERS=0; [[ "${1:-}" == "--no-containers" ]] && NO_CONTAINERS=1
log() { printf '\n\033[1;36m========== %s ==========\033[0m\n' "$*"; }

log "Tier 0: clippy (-D warnings)"
cargo clippy --workspace --all-targets -- -D warnings

log "Tier 0+1: workspace tests (unit + gateway integration incl. observability + databridge_engines)"
cargo test --workspace

log "Tier 0: optional-feature compile checks"
for f in sqlserver oracle mongodb; do
  echo "--- --features $f ---"; cargo build -p atlas-databridge --features "$f"
done
if command -v cmake >/dev/null; then
  echo "--- --features kafka-lag ---"; cargo build -p atlas-databridge --features kafka-lag
else
  echo "!! skipping kafka-lag compile (cmake not installed)"
fi

# NOTE: `cargo fmt --all --check` is intentionally NOT run here. The repo's committed style predates
# the current stable rustfmt and the fmt gate is red project-wide (independent of these tests); running
# it locally with a mismatched rustfmt would produce a misleading, whole-tree diff.

if [[ "$NO_CONTAINERS" == "0" ]] && { command -v podman >/dev/null || command -v docker >/dev/null; }; then
  log "Tier 2: real DB connectors via containers"
  ./scripts/test-connectors.sh
else
  echo "!! skipping Tier 2 (no container runtime, or --no-containers)"
fi

log "DONE. Opt-in follow-ups:"
cat <<'EOF'
  Tier 3 (real Ceph/k8s write path): redeploy the gateway on a k3s+Rook host in real mode, then
          POST /volumes and confirm the PVC binds on zyvor-rbd-prod (mutates the remote).
  Tier 4 (UI smoke): `make ui` then GET / and confirm the real console bundle serves (needs npm).
EOF
