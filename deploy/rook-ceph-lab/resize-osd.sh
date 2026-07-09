#!/usr/bin/env bash
# Copyright (c) 2026 ZyvorAI Labs Private Limited. All rights reserved.
#
# resize-osd.sh — cap Ceph to a fixed slice of the OSD disk (default 400 GiB).
#
# A BlueStore OSD on a raw device claims the WHOLE device (here /dev/sdb = 931 GiB). There is no
# online shrink; to "use only 400 GB for Ceph" you must recreate the OSD on a 400 GiB PARTITION of
# the disk, freeing the rest of /dev/sdb for other use. This script does that.
#
# DESTRUCTIVE: destroys and rebuilds the OSD → ALL CEPH DATA IS LOST (RBD, CephFS, RGW).
# The rook-ceph operator + CRDs must already be installed (from up.sh).
#
#   ./resize-osd.sh 400 --confirm      # rebuild the OSD on a 400 GiB partition of /dev/sdb
#   ./resize-osd.sh                    # dry-run: print the plan, change nothing
#
set -euo pipefail
HERE="$(cd "$(dirname "$0")" && pwd)"
NS=rook-ceph
DEV="${CEPH_DEVICE:-sdb}"
SIZE_GIB="${1:-400}"
[[ "$SIZE_GIB" =~ ^[0-9]+$ ]] || { echo "size must be an integer number of GiB"; exit 1; }
PART="${DEV}1"
say() { printf '\033[1;36m==>\033[0m %s\n' "$*"; }
warn() { printf '\033[1;33m!! \033[0m %s\n' "$*"; }

if [ "${2:-}" != "--confirm" ]; then
  cat <<EOF
DRY-RUN. This would REBUILD the Ceph OSD capped to ${SIZE_GIB} GiB, DESTROYING ALL CEPH DATA:
  1. arm Rook disk-cleanup + delete the CephCluster (releases + zaps /dev/$DEV)
  2. GPT-partition /dev/$DEV → /dev/$PART = ${SIZE_GIB} GiB (rest of the disk left free)
  3. re-create the CephCluster with device '$PART' (1 mon/mgr, osd failure domain)
  4. re-apply the single-node RBD pool / CephFS / RGW + StorageClasses
Re-run with:  ./resize-osd.sh ${SIZE_GIB} --confirm
EOF
  exit 0
fi

kubectl -n "$NS" get deploy rook-ceph-operator >/dev/null 2>&1 || {
  echo "rook-ceph-operator not found — run ./up.sh first"; exit 1; }

warn "REBUILDING Ceph OSD on /dev/$DEV capped to ${SIZE_GIB} GiB — ALL CEPH DATA LOST — 5s to abort"; sleep 5

say "1/5 arm disk-cleanup + delete CephCluster (Rook zaps /dev/$DEV)"
kubectl -n "$NS" patch cephcluster rook-ceph --type merge \
  -p '{"spec":{"cleanupPolicy":{"confirmation":"yes-really-destroy-data"}}}' || true
kubectl -n "$NS" delete cephcluster rook-ceph --ignore-not-found --timeout=300s || true
# wait for OSD/mon pods to drain
for i in $(seq 1 30); do
  n=$(kubectl -n "$NS" get pods --no-headers 2>/dev/null | grep -cE 'rook-ceph-(osd|mon|mgr)' || true)
  [ "$n" = "0" ] && break; sleep 5
done

say "2/5 partition /dev/$DEV → /dev/$PART = ${SIZE_GIB} GiB"
sudo bash -c '
  set -e
  for m in $(dmsetup ls 2>/dev/null | awk "/ceph/{print \$1}"); do dmsetup remove "$m" || true; done
  sgdisk --zap-all "/dev/'"$DEV"'"
  wipefs -a "/dev/'"$DEV"'" || true
  parted -s "/dev/'"$DEV"'" mklabel gpt
  parted -s "/dev/'"$DEV"'" mkpart primary 1MiB "'"${SIZE_GIB}"'GiB"
  partprobe "/dev/'"$DEV"'"
  sleep 2
  lsblk -o NAME,SIZE,TYPE "/dev/'"$DEV"'"
'

say "3/5 re-create the CephCluster on the ${SIZE_GIB} GiB partition /dev/$PART"
NODE="$(kubectl get nodes -o jsonpath='{.items[0].metadata.name}')"
kubectl apply -f - <<YAML
apiVersion: ceph.rook.io/v1
kind: CephCluster
metadata:
  name: rook-ceph
  namespace: $NS
spec:
  cephVersion:
    image: quay.io/ceph/ceph:v18.2.4
    allowUnsupported: true
  dataDirHostPath: /var/lib/rook
  mon: { count: 1, allowMultiplePerNode: true }
  mgr:
    count: 1
    modules: [ { name: prometheus, enabled: true } ]
  dashboard: { enabled: false }
  monitoring: { enabled: false }
  network: { connections: { encryption: { enabled: false } } }
  storage:
    useAllNodes: false
    useAllDevices: false
    nodes:
      - name: $NODE
        devices:
          - name: $PART      # capped to ${SIZE_GIB} GiB (rest of /dev/$DEV is free)
  disruptionManagement: { managePodBudgets: false }
YAML

say "waiting for the new OSD to come up (up to 5 min)"
kubectl -n "$NS" wait --for=condition=Ready pod -l app=rook-ceph-osd --timeout=300s || warn "OSD not Ready yet — check 'kubectl -n $NS get pods'"

say "4/5 re-apply pools / CephFS / RGW + StorageClasses"
kubectl apply -f "$HERE/single-node/blockpool-sc.yaml"
kubectl apply -f "$HERE/single-node/cephfs-sc.yaml"
kubectl apply -f "$HERE/single-node/rgw.yaml"

say "5/5 verify"
kubectl -n "$NS" get cephcluster rook-ceph -o jsonpath='{.status.ceph.health}{"\n"}' || true
echo "Ceph is now capped to ${SIZE_GIB} GiB on /dev/$PART; the remainder of /dev/$DEV is free."
