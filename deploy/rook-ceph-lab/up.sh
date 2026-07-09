#!/usr/bin/env bash
# Copyright (c) 2026 ZyvorAI Labs Private Limited. All rights reserved.
#
# Stand up the Atlas lab storage stack on an existing Kubernetes/K3s cluster:
#   Rook operator -> CephCluster -> RBD/CephFS/RGW + StorageClasses -> snapshotter
#   -> (optional) KubeVirt + CDI -> (optional) sample VM on a Ceph-backed PVC.
#
# Requires: kubectl (pointing at the target cluster), and empty block devices on the nodes
# for Ceph OSDs. Idempotent: safe to re-run.
#
# Usage:
#   ./up.sh                 # rook + ceph + storage classes + snapshotter
#   ./up.sh --kubevirt      # also install KubeVirt + CDI
#   ./up.sh --sample-vm     # also apply the sample DataVolume + VM (implies --kubevirt)
#   ROOK_VERSION=v1.15.6 KUBEVIRT_VERSION=v1.3.1 CDI_VERSION=v1.60.3 ./up.sh
set -euo pipefail

ROOK_VERSION="${ROOK_VERSION:-v1.15.6}"
KUBEVIRT_VERSION="${KUBEVIRT_VERSION:-v1.3.1}"
CDI_VERSION="${CDI_VERSION:-v1.60.3}"
SNAPSHOTTER_VERSION="${SNAPSHOTTER_VERSION:-v8.1.0}"
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

WITH_KUBEVIRT=0
WITH_SAMPLE_VM=0
for arg in "$@"; do
  case "$arg" in
    --kubevirt) WITH_KUBEVIRT=1 ;;
    --sample-vm) WITH_KUBEVIRT=1; WITH_SAMPLE_VM=1 ;;
    *) echo "unknown flag: $arg" >&2; exit 2 ;;
  esac
done

log() { printf '\033[1;36m==> %s\033[0m\n' "$*"; }

require() { command -v "$1" >/dev/null 2>&1 || { echo "missing required tool: $1" >&2; exit 1; }; }
require kubectl

log "1/7 namespaces"
kubectl apply -f "$HERE/01-namespaces.yaml"

log "2/7 Rook Ceph operator ($ROOK_VERSION)"
# Rook release BRANCHES have no leading 'v' (release-1.15); only the tags do (v1.15.6).
rook_minor="${ROOK_VERSION#v}"; rook_minor="${rook_minor%.*}"   # v1.15.6 -> 1.15
base="https://raw.githubusercontent.com/rook/rook/release-${rook_minor}/deploy/examples"
kubectl apply -f "$base/crds.yaml"
kubectl apply -f "$base/common.yaml"
kubectl apply -f "$base/operator.yaml"
log "waiting for rook-ceph-operator to be ready..."
kubectl -n rook-ceph rollout status deploy/rook-ceph-operator --timeout=300s

log "3/7 CephCluster (this can take several minutes to reach HEALTH_OK)"
kubectl apply -f "$HERE/02-cephcluster.yaml"

log "4/7 external-snapshotter ($SNAPSHOTTER_VERSION)"
snap="https://raw.githubusercontent.com/kubernetes-csi/external-snapshotter/${SNAPSHOTTER_VERSION}"
kubectl apply -f "$snap/client/config/crd/snapshot.storage.k8s.io_volumesnapshotclasses.yaml"
kubectl apply -f "$snap/client/config/crd/snapshot.storage.k8s.io_volumesnapshotcontents.yaml"
kubectl apply -f "$snap/client/config/crd/snapshot.storage.k8s.io_volumesnapshots.yaml"
kubectl -n kube-system apply -f "$snap/deploy/kubernetes/snapshot-controller/rbac-snapshot-controller.yaml"
kubectl -n kube-system apply -f "$snap/deploy/kubernetes/snapshot-controller/setup-snapshot-controller.yaml"

log "5/7 pools, filesystem, object store, storage classes, snapshot class"
kubectl apply -f "$HERE/03-rbd-blockpool-and-storageclass.yaml"
kubectl apply -f "$HERE/04-cephfs-and-storageclass.yaml"
kubectl apply -f "$HERE/05-rgw-objectstore.yaml"
kubectl apply -f "$HERE/06-volumesnapshotclass.yaml"

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
