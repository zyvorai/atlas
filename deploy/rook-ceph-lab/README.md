# Atlas lab — Rook Ceph + KubeVirt

Stands up the storage fabric Atlas manages, on an existing Kubernetes/K3s cluster (PDF §6.1).

## Prerequisites
- A reachable cluster (`kubectl` context set).
- Empty block devices on the nodes for Ceph OSDs (`useAllDevices: true` in `02-cephcluster.yaml`
  consumes any unmounted device — pin `nodes`/`devices` for anything but a throwaway lab).
- 3 nodes recommended; `allowMultiplePerNode` lets mons co-locate for single/dual-node labs.

## Install
```bash
./up.sh              # Rook + Ceph + RBD/CephFS/RGW StorageClasses + snapshotter
./up.sh --kubevirt   # also KubeVirt + CDI
./up.sh --sample-vm  # also a VM booting from a zyvor-rbd-prod PVC
```
Pin versions via env: `ROOK_VERSION`, `KUBEVIRT_VERSION`, `CDI_VERSION`, `SNAPSHOTTER_VERSION`.

## What you get
| Object | Name |
|---|---|
| CephCluster | `rook-ceph` (3 mon, mgr + prometheus module) |
| RBD pool + StorageClass | `rbd-nvme-prod` / `zyvor-rbd-prod` |
| CephFS + RWX StorageClass | `zyvorfs` / `zyvor-cephfs-shared` |
| RGW object store + bucket class | `zyvor-rgw` / `zyvor-rgw-bucket` |
| VolumeSnapshotClass | `zyvor-rbd-snapclass` |

## Verify
```bash
kubectl -n rook-ceph get cephcluster          # PHASE should reach Ready / HEALTH_OK
kubectl get storageclass | grep zyvor
kubectl get volumesnapshotclass
atlasctl storage-classes                       # Atlas live k8s driver lists the zyvor-* classes
```

## Lint without applying
```bash
for f in *.yaml; do kubectl apply --dry-run=client -f "$f"; done
```
Note: `--dry-run=client` on the Ceph CRs requires the Rook CRDs to be installed first
(`kubectl apply -f .../crds.yaml`), otherwise the CephCluster/CephBlockPool kinds are unknown.
