#!/usr/bin/env bash
# Copyright (c) 2026 ZyvorAI Labs Private Limited. All rights reserved.
#
# Deploy the Atlas gateway to a remote k3s host (same convention as the other Zyvor projects:
# `./scripts/deploy-remote.sh <host> <user>`).
#
# What it does:
#   1. rsync the repo to <user>@<host>:~/.deployment/atlas
#   2. build the atlas-gateway image on the remote with podman (no local Rust toolchain needed)
#   3. import the image into k3s containerd
#   4. kubectl apply deploy/k8s/atlas-gateway.yaml (+ RBAC + NodePort 30510), then rollout restart so
#      the freshly-imported same-tag (:dev) image actually rolls out
#   5. verify /health and /storage-classes over the NodePort
#
# Optional:
#   --with-ceph      also run deploy/rook-ceph-lab/up.sh on the remote (CONSUMES an empty disk
#                    for a Ceph OSD — destructive to that disk; see up.sh). Off by default.
#   --with-k3s-disk  also run deploy/rook-ceph-lab/setup-k3s-disk.sh: carve /dev/sdb2 from the
#                    free tail left by resize-osd.sh and move the k3s data-dir onto it, so the
#                    big disk carries the k3s load instead of the small root FS. Off by default.
#
# Day-2 upgrade orchestration:
#   --rollback       revert the gateway Deployment to its previous ReplicaSet (`kubectl rollout undo`)
#                    instead of building/deploying — for a bad upgrade. Skips build/import.
#   --force          proceed even if the upgrade pre-flight (`GET /upgrade/preflight`) reports blockers.
#
# Usage:
#   ./scripts/deploy-remote.sh 212.8.248.187 sus
#   ./scripts/deploy-remote.sh 212.8.248.187 sus --with-ceph
#   ./scripts/deploy-remote.sh 212.8.248.187 sus --rollback
set -euo pipefail

HOST="${1:-${DEPLOY_HOST:-}}"
USER="${2:-${DEPLOY_USER:-sus}}"
WITH_CEPH=0
WITH_K3S_DISK=0
ROLLBACK=0
FORCE=0
for a in "$@"; do
  [[ "$a" == "--with-ceph" ]] && WITH_CEPH=1
  [[ "$a" == "--with-k3s-disk" ]] && WITH_K3S_DISK=1
  [[ "$a" == "--rollback" ]] && ROLLBACK=1
  [[ "$a" == "--force" ]] && FORCE=1
done
[[ -z "$HOST" ]] && { echo "usage: $0 <host> <user> [--with-ceph] [--with-k3s-disk] [--rollback] [--force]" >&2; exit 2; }

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
REMOTE_DIR=".deployment/atlas"
SSH="ssh -o StrictHostKeyChecking=accept-new ${USER}@${HOST}"
NS="zyvor-system"
DEPLOY="atlas-gateway"
NODEPORT=30510

log()  { printf '\033[1;36m==> %s\033[0m\n' "$*"; }
warn() { printf '\033[1;33m!! %s\033[0m\n' "$*"; }

# --rollback: revert to the previous ReplicaSet and exit (skip build/import).
if [[ "$ROLLBACK" == "1" ]]; then
  log "rollback: reverting ${DEPLOY} to its previous revision"
  $SSH "kubectl -n ${NS} rollout undo deploy/${DEPLOY} && kubectl -n ${NS} rollout status deploy/${DEPLOY} --timeout=180s"
  $SSH "curl -fsS http://127.0.0.1:${NODEPORT}/version; echo" || true
  log "rollback done."
  exit 0
fi

# Pre-flight: if the gateway is already up, gate the upgrade on its readiness (no upgrade mid-incident).
if $SSH "curl -fsS http://127.0.0.1:${NODEPORT}/api/atlas/v1/upgrade/preflight" >/tmp/atlas-preflight.json 2>/dev/null; then
  if grep -q '"ready":false' /tmp/atlas-preflight.json; then
    warn "upgrade pre-flight reported blockers:"; cat /tmp/atlas-preflight.json; echo
    [[ "$FORCE" == "1" ]] || { warn "aborting (pass --force to override, or drain via POST /maintenance)"; exit 3; }
    warn "--force set; proceeding despite blockers"
  else
    log "upgrade pre-flight: ready"
  fi
fi
rm -f /tmp/atlas-preflight.json

log "1/5 rsync repo -> ${USER}@${HOST}:~/${REMOTE_DIR}"
$SSH "mkdir -p ~/${REMOTE_DIR}"
rsync -az --delete \
  --exclude target --exclude .git --exclude '*.db' --exclude '*.db-wal' --exclude '*.db-shm' \
  -e "ssh -o StrictHostKeyChecking=accept-new" \
  "${HERE}/" "${USER}@${HOST}:${REMOTE_DIR}/"

log "2/5 build image with podman on remote"
$SSH "cd ~/${REMOTE_DIR} && podman build -t atlas-gateway:dev -f Dockerfile ."

log "3/5 import image into k3s containerd"
$SSH "cd ~/${REMOTE_DIR} && podman save atlas-gateway:dev -o /tmp/atlas-gateway.tar && sudo k3s ctr images import /tmp/atlas-gateway.tar && rm -f /tmp/atlas-gateway.tar"

log "4/5 apply k8s manifests + roll out the new image"
# `kubectl apply` is a no-op when only the image *content* changed (the tag stays :dev), so it won't
# restart the pod and the old build keeps serving. `rollout restart` stamps the pod template so a new
# pod always comes up on the freshly-imported containerd image (imagePullPolicy: Never).
$SSH "cd ~/${REMOTE_DIR} \
  && kubectl apply -f deploy/k8s/atlas-gateway.yaml \
  && kubectl -n zyvor-system rollout restart deploy/atlas-gateway \
  && kubectl -n zyvor-system rollout status deploy/atlas-gateway --timeout=180s"

if [[ "$WITH_CEPH" == "1" ]]; then
  log "4b/5 installing Rook Ceph (DESTRUCTIVE: consumes an empty disk as an OSD)"
  $SSH "cd ~/${REMOTE_DIR} && bash deploy/rook-ceph-lab/up.sh"
fi

if [[ "$WITH_K3S_DISK" == "1" ]]; then
  log "4c/5 carving /dev/sdb2 + moving the k3s data-dir onto it (needs sdb1 from resize-osd.sh)"
  $SSH "cd ~/${REMOTE_DIR} && bash deploy/rook-ceph-lab/setup-k3s-disk.sh --confirm"
fi

log "5/5 verify over NodePort ${NODEPORT}"
$SSH "set -e
  echo '--- /health ---'; curl -fsS http://127.0.0.1:${NODEPORT}/health; echo
  echo '--- /version ---'; curl -fsS http://127.0.0.1:${NODEPORT}/version; echo
  echo '--- trigger discovery ---'; curl -fsS -X POST http://127.0.0.1:${NODEPORT}/api/atlas/v1/backends/bkd_ceph_lab/discover; echo
  echo '--- /pools ---'; curl -fsS http://127.0.0.1:${NODEPORT}/api/atlas/v1/pools; echo
  echo '--- /storage-classes (LIVE from k3s) ---'; curl -fsS http://127.0.0.1:${NODEPORT}/api/atlas/v1/storage-classes; echo
"

log "done. Atlas gateway on http://${HOST}:${NODEPORT}"
