#!/usr/bin/env bash
# Copyright (c) 2026 ZyvorAI Labs Private Limited.
# SPDX-License-Identifier: AGPL-3.0-only OR LicenseRef-Atlas-Commercial
# Stand up the Atlas lab storage stack on an existing Kubernetes/K3s cluster:
#   Rook operator -> CephCluster -> RBD/CephFS/RGW + StorageClasses -> snapshotter
#   -> (optional) KubeVirt + CDI -> (optional) sample VM on a Ceph-backed PVC.
#
# Requires: kubectl (pointing at the target cluster), and empty block devices on the nodes
# for Ceph OSDs. Idempotent: safe to re-run.
#
# Usage:
#   ./up.sh                 # rook + ceph + storage classes + snapshotter
#   ./up.sh --single-node   # 1-mon / size=1 overlay (lab on one k3s node + /dev/sdb)
#   ./up.sh --kubevirt      # also install KubeVirt + CDI
#   ./up.sh --sample-vm     # also apply the sample DataVolume + VM (implies --kubevirt)
#   ROOK_VERSION=v1.20.2 CEPH_IMAGE=quay.io/ceph/ceph:v19.2.3 ./up.sh --single-node
#
# If the Rook operator is already installed (e.g. via hypercluster `storage apply`),
# pass --cluster-only to skip operator/CRD install and only apply the CephCluster + SCs.
set -euo pipefail

ROOK_VERSION="${ROOK_VERSION:-v1.20.2}"
KUBEVIRT_VERSION="${KUBEVIRT_VERSION:-v1.3.1}"
CDI_VERSION="${CDI_VERSION:-v1.60.3}"
SNAPSHOTTER_VERSION="${SNAPSHOTTER_VERSION:-v8.1.0}"
CEPH_IMAGE="${CEPH_IMAGE:-quay.io/ceph/ceph:v19.2.3}"
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

WITH_KUBEVIRT=0
WITH_SAMPLE_VM=0
SINGLE_NODE=0
CLUSTER_ONLY=0
for arg in "$@"; do
  case "$arg" in
    --kubevirt) WITH_KUBEVIRT=1 ;;
    --sample-vm) WITH_KUBEVIRT=1; WITH_SAMPLE_VM=1 ;;
    --single-node) SINGLE_NODE=1 ;;
    --cluster-only) CLUSTER_ONLY=1 ;;
    *) echo "unknown flag: $arg" >&2; exit 2 ;;
  esac
done

log() { printf '\033[1;36m==> %s\033[0m\n' "$*"; }

require() { command -v "$1" >/dev/null 2>&1 || { echo "missing required tool: $1" >&2; exit 1; }; }
require kubectl

log "1/7 namespaces"
kubectl apply -f "$HERE/01-namespaces.yaml"

if [[ "$CLUSTER_ONLY" != "1" ]]; then
  log "2/7 Rook Ceph operator ($ROOK_VERSION)"
  # Rook release BRANCHES have no leading 'v' (release-1.15); only the tags do (v1.15.6).
  rook_minor="${ROOK_VERSION#v}"; rook_minor="${rook_minor%.*}"   # v1.20.2 -> 1.20
  base="https://raw.githubusercontent.com/rook/rook/release-${rook_minor}/deploy/examples"
  kubectl apply -f "$base/crds.yaml"
  kubectl apply -f "$base/common.yaml"
  kubectl apply -f "$base/operator.yaml"
  log "waiting for rook-ceph-operator to be ready..."
  kubectl -n rook-ceph rollout status deploy/rook-ceph-operator --timeout=300s
else
  log "2/7 skipping Rook operator install (--cluster-only; using existing operator)"
  kubectl -n rook-ceph rollout status deploy/rook-ceph-operator --timeout=300s
fi

log "3/7 CephCluster (image=$CEPH_IMAGE; this can take several minutes to reach Ready)"
if [[ "$SINGLE_NODE" == "1" ]]; then
  # Render image pin into the single-node CR so CEPH_IMAGE overrides stay in the script.
  tmp="$(mktemp)"
  sed "s|image: quay.io/ceph/ceph:.*|image: ${CEPH_IMAGE}|" \
    "$HERE/single-node/cluster.yaml" >"$tmp"
  kubectl apply -f "$tmp"
  rm -f "$tmp"
else
  tmp="$(mktemp)"
  sed "s|image: quay.io/ceph/ceph:.*|image: ${CEPH_IMAGE}|" \
    "$HERE/02-cephcluster.yaml" >"$tmp"
  kubectl apply -f "$tmp"
  rm -f "$tmp"
fi

log "waiting for CephCluster Ready (up to 20m)..."
# Re-applying the CR can briefly leave a stale Ready; require two consecutive Ready polls.
ok=0
ready_streak=0
for _ in $(seq 1 80); do
  phase="$(kubectl -n rook-ceph get cephcluster rook-ceph -o jsonpath='{.status.phase}' 2>/dev/null || true)"
  if [[ "$phase" == "Ready" ]]; then
    ready_streak=$((ready_streak + 1))
    if [[ "$ready_streak" -ge 2 ]]; then ok=1; break; fi
  else
    ready_streak=0
  fi
  sleep 15
done
if [[ "$ok" != "1" ]]; then
  kubectl -n rook-ceph get cephcluster,pods,jobs
  echo "CephCluster did not reach Ready in time" >&2
  exit 1
fi

# Rook v1.20+ requires the companion ceph-csi-drivers chart (Driver CRs + SAs).
# Without it, zyvor-* StorageClasses never provision PVCs.
log "3b/7 Ceph CSI drivers (rook-prefixed names for Atlas StorageClasses)"
if command -v helm >/dev/null 2>&1; then
  helm repo add ceph-csi-operator https://ceph.github.io/ceph-csi-operator 2>/dev/null || true
  helm repo update ceph-csi-operator >/dev/null 2>&1 || true
  csi_vals="$(mktemp)"
  cat >"$csi_vals" <<'EOF'
drivers:
  rbd:
    name: rook-ceph.rbd.csi.ceph.com
    enabled: true
  cephfs:
    name: rook-ceph.cephfs.csi.ceph.com
    enabled: true
  nfs:
    enabled: false
EOF
  helm upgrade --install ceph-csi-drivers ceph-csi-operator/ceph-csi-drivers \
    --namespace rook-ceph \
    --values "$csi_vals" \
    --wait --timeout 10m
  rm -f "$csi_vals"
else
  echo "helm not found — install helm and re-run, or: helm upgrade --install ceph-csi-drivers ceph-csi-operator/ceph-csi-drivers -n rook-ceph" >&2
  exit 1
fi

if [[ "$CLUSTER_ONLY" != "1" ]]; then
  log "4/7 external-snapshotter ($SNAPSHOTTER_VERSION)"
  snap="https://raw.githubusercontent.com/kubernetes-csi/external-snapshotter/${SNAPSHOTTER_VERSION}"
  kubectl apply -f "$snap/client/config/crd/snapshot.storage.k8s.io_volumesnapshotclasses.yaml"
  kubectl apply -f "$snap/client/config/crd/snapshot.storage.k8s.io_volumesnapshotcontents.yaml"
  kubectl apply -f "$snap/client/config/crd/snapshot.storage.k8s.io_volumesnapshots.yaml"
  kubectl -n kube-system apply -f "$snap/deploy/kubernetes/snapshot-controller/rbac-snapshot-controller.yaml"
  kubectl -n kube-system apply -f "$snap/deploy/kubernetes/snapshot-controller/setup-snapshot-controller.yaml"
else
  log "4/7 skipping snapshotter (--cluster-only)"
fi

log "5/7 pools, filesystem, object store, storage classes, snapshot class"
if [[ "$SINGLE_NODE" == "1" ]]; then
  kubectl apply -f "$HERE/single-node/blockpool-sc.yaml"
  kubectl apply -f "$HERE/single-node/cephfs-sc.yaml"
  kubectl apply -f "$HERE/single-node/rgw.yaml"
  kubectl apply -f "$HERE/06-volumesnapshotclass.yaml"
else
  kubectl apply -f "$HERE/03-rbd-blockpool-and-storageclass.yaml"
  kubectl apply -f "$HERE/04-cephfs-and-storageclass.yaml"
  kubectl apply -f "$HERE/05-rgw-objectstore.yaml"
  kubectl apply -f "$HERE/06-volumesnapshotclass.yaml"
fi

if [[ "$WITH_KUBEVIRT" == "1" ]]; then
  log "6/7 KubeVirt ($KUBEVIRT_VERSION) + CDI ($CDI_VERSION)"
  kubectl apply -f "https://github.com/kubevirt/kubevirt/releases/download/${KUBEVIRT_VERSION}/kubevirt-operator.yaml"
  kubectl apply -f "https://github.com/kubevirt/kubevirt/releases/download/${KUBEVIRT_VERSION}/kubevirt-cr.yaml"
  kubectl apply -f "https://github.com/kubevirt/containerized-data-importer/releases/download/${CDI_VERSION}/cdi-operator.yaml"
  kubectl apply -f "https://github.com/kubevirt/containerized-data-importer/releases/download/${CDI_VERSION}/cdi-cr.yaml"
else
  log "6/7 skipping KubeVirt/CDI (pass --kubevirt to install)"
fi

if [[ "$WITH_SAMPLE_VM" == "1" ]]; then
  log "7/7 sample VM on Ceph-backed PVC"
  kubectl apply -f "$HERE/07-sample-kubevirt-vm.yaml"
else
  log "7/7 skipping sample VM (pass --sample-vm to apply)"
fi

cat <<EOF

Done. Verify with:
  kubectl -n rook-ceph get cephcluster
  kubectl get storageclass | grep zyvor
  kubectl get volumesnapshotclass

Then point Atlas at this cluster and discover:
  ATLAS_CEPH_DRIVER_MODE=real atlas-gateway         # (needs ceph/rbd access), or
  atlasctl storage-classes                          # live k8s driver lists the zyvor-* classes
EOF
