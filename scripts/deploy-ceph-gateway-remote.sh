#!/usr/bin/env bash
# Copyright (c) 2026 ZyvorAI Labs Private Limited. All rights reserved.
#
# Deploy the **real-Ceph** Atlas gateway (`atlas-gateway-ceph` in `rook-ceph`) to a
# remote k3s host. The sibling `deploy-remote.sh` builds the fake/k8s image
# (`Dockerfile`) and deploys to `zyvor-system`; THIS one builds `Dockerfile.ceph`
# (bundles the Ceph Reef client) and rolls out the rook-ceph deployment, whose pod
# runs `localhost/atlas-gateway:ceph` with `imagePullPolicy: Never`.
#
# What it does:
#   1. rsync the repo to <user>@<host>:~/.deployment/atlas
#   2. podman build -t atlas-gateway:ceph -f Dockerfile.ceph .
#   3. import the image into k3s containerd
#   4. kubectl apply deploy/k8s/atlas-gateway-ceph.yaml + rollout restart/status
#
# Usage:
#   ./scripts/deploy-ceph-gateway-remote.sh <host> [user]
#   ./scripts/deploy-ceph-gateway-remote.sh 212.8.248.187 sus
#
# Env:
#   DEPLOY_HOST / DEPLOY_USER   Defaults when host/user omitted
#   ATLAS_NS                    Gateway namespace       (default rook-ceph)
#   ATLAS_DEPLOY                Deployment name         (default atlas-gateway-ceph)
#   ATLAS_SKIP_APPLY=1          Skip `kubectl apply` of the manifest (rollout only)
#   ATLAS_SSH_TIMEOUT           ssh ConnectTimeout secs (default 90)
set -euo pipefail

HOST="${1:-${DEPLOY_HOST:-}}"
USER="${2:-${DEPLOY_USER:-sus}}"
[[ -z "${HOST}" ]] && { echo "usage: $0 <host> [user]" >&2; exit 2; }

NS="${ATLAS_NS:-rook-ceph}"
DEPLOY="${ATLAS_DEPLOY:-atlas-gateway-ceph}"
MANIFEST="deploy/k8s/atlas-gateway-ceph.yaml"
REMOTE_DIR=".deployment/atlas"
TIMEOUT="${ATLAS_SSH_TIMEOUT:-90}"
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SSH="ssh -o StrictHostKeyChecking=accept-new -o ConnectTimeout=${TIMEOUT} ${USER}@${HOST}"

log() { printf '\033[1;36m==> %s\033[0m\n' "$*"; }

log "1/4 rsync repo -> ${USER}@${HOST}:~/${REMOTE_DIR}"
${SSH} "mkdir -p ~/${REMOTE_DIR}"
rsync -az --delete \
  --exclude target --exclude .git --exclude '*.db' --exclude '*.db-wal' --exclude '*.db-shm' \
  --exclude node_modules \
  -e "ssh -o StrictHostKeyChecking=accept-new -o ConnectTimeout=${TIMEOUT}" \
  "${HERE}/" "${USER}@${HOST}:${REMOTE_DIR}/"

log "2/4 build atlas-gateway:ceph (Dockerfile.ceph) with podman on remote"
${SSH} "cd ~/${REMOTE_DIR} && podman build -t atlas-gateway:ceph -f Dockerfile.ceph ."

log "3/6 import image into k3s containerd"
${SSH} "cd ~/${REMOTE_DIR} && podman save atlas-gateway:ceph -o /tmp/atlas-gateway-ceph.tar \
  && sudo k3s ctr images import /tmp/atlas-gateway-ceph.tar && rm -f /tmp/atlas-gateway-ceph.tar"

# ${MANIFEST}'s own ClusterRoleBinding (atlas-gateway-readonly-cephns) already
# targets the right ServiceAccount (rook-ceph:atlas-gateway) — it just
# references a ClusterRole named atlas-gateway-readonly that this script never
# created, because that ClusterRole is defined in the *sibling* deploy-remote.sh's
# manifest (deploy/k8s/atlas-gateway.yaml), which this script doesn't apply. On a
# cluster that only ever ran the real-Ceph path, the pod started with the
# ClusterRole missing and every k8s API call it made came back "clusterrole ...
# not found: Forbidden" — non-fatal (only the live k8s driver's PV/PVC reads
# degrade) but silent, and easy to miss on a fresh install.
log "4/6 ensure the shared atlas-gateway-readonly ClusterRole exists"
${SSH} bash -s <<REMOTE
set -euo pipefail
cd ~/${REMOTE_DIR}
rm -rf /tmp/atlas-rbac-split
mkdir -p /tmp/atlas-rbac-split
csplit -z -s -f /tmp/atlas-rbac-split/doc- deploy/k8s/atlas-gateway.yaml '/^---\$/' '{*}'
for f in /tmp/atlas-rbac-split/doc-*; do
  if grep -q '^kind: ClusterRole\$' "\$f"; then
    sudo kubectl apply -f "\$f"
  fi
done
rm -rf /tmp/atlas-rbac-split
REMOTE

# ${MANIFEST}'s Deployment mounts a `tls` volume from a Secret named atlas-tls
# (deploy/k8s/atlas-gateway-ceph.yaml) that nothing creates — on a fresh
# install the pod crash-loops on "load TLS cert/key: ... No such file or
# directory" until someone notices and creates it by hand. Self-signed and
# idempotent: left alone once it exists, so re-running this script never
# rotates a cert a browser has already been told to trust.
log "5/6 ensure the atlas-tls secret exists (self-signed, HTTPS listener)"
${SSH} bash -s <<REMOTE
set -euo pipefail
if ! sudo kubectl -n ${NS} get secret atlas-tls >/dev/null 2>&1; then
  tmpdir="\$(mktemp -d)"
  trap 'rm -rf "\$tmpdir"' EXIT
  openssl req -x509 -newkey rsa:2048 -nodes \
    -keyout "\$tmpdir/tls.key" -out "\$tmpdir/tls.crt" \
    -days 365 -subj "/CN=atlas-gateway" >/dev/null 2>&1
  sudo kubectl -n ${NS} create secret tls atlas-tls \
    --cert="\$tmpdir/tls.crt" --key="\$tmpdir/tls.key"
else
  echo "atlas-tls already exists — left alone"
fi
REMOTE

log "6/6 apply manifest + roll out ${DEPLOY} in ${NS}"
if [[ "${ATLAS_SKIP_APPLY:-0}" != "1" ]]; then
  ${SSH} "cd ~/${REMOTE_DIR} && sudo kubectl apply -f ${MANIFEST}"
fi
${SSH} "sudo kubectl -n ${NS} rollout restart deploy/${DEPLOY} \
  && sudo kubectl -n ${NS} rollout status deploy/${DEPLOY} --timeout=180s"

log "done — verify: curl -s http://<node>:30511/api/atlas/v1/volumes?kind=block"
