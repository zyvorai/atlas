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

## Operations scripts

Day-2 helpers for the lab Ceph on `212.8.248.187` (OSD lives on the dedicated disk `/dev/sdb`;
the root FS `/dev/sda2` is separate):

- **`reclaim-space.sh`** — *safe, non-destructive.* Reclaims ROOT-disk space eaten by repeated
  `podman build` layers + `k3s ctr images import` on every deploy, plus journals. Ceph untouched.
  ```sh
  ./reclaim-space.sh
  ```
- **`resize-osd.sh [GiB] --confirm`** — cap Ceph to a fixed slice of the OSD disk (default **400 GiB**)
  by rebuilding the OSD on a `/dev/sdb1` partition, freeing the rest of the disk. **Destructive**
  (rebuilds the OSD → all Ceph data lost). Dry-run without `--confirm`.
  ```sh
  ./resize-osd.sh 400 --confirm
  ```
- **`setup-k3s-disk.sh --confirm`** — put the **k3s storage load on the big disk.** After
  `resize-osd.sh` caps Ceph to `/dev/sdb1` (400 GiB), ~531 GiB of `/dev/sdb` is left free; this
  carves it into `/dev/sdb2`, formats it, and moves the k3s data-dir (`/var/lib/rancher` —
  containerd image store + local-path PV data + datastore) onto it so k3s stops filling the small
  root FS (`/dev/sda2`). Creates **only** the new partition — the Ceph OSD on `/dev/sdb1` is left
  intact, so it's safe on a live cluster. Stops/starts k3s around the move; keeps the pre-move copy
  at `/var/lib/rancher.pre-sdb2`. Dry-run without `--confirm`.
  ```sh
  ./resize-osd.sh 400 --confirm     # first: Ceph -> /dev/sdb1, free the rest of /dev/sdb
  ./setup-k3s-disk.sh --confirm     # then: /dev/sdb2 <- the free tail, k3s data-dir moves there
  ```
- **`teardown.sh --confirm`** — fully uninstall Rook Ceph (reverses `up.sh`) and wipe `/dev/sdb`
  using Rook's own `cleanupPolicy`. **Destructive.** Dry-run without `--confirm`.
  ```sh
  ./teardown.sh --confirm      # then ./up.sh to reinstall
  ```

> Note: the OSD claims the **whole** raw device, so a BlueStore OSD can't be shrunk online — capping
> to 400 GiB means recreating it on a partition, which is why `resize-osd.sh` is destructive.
> After that, `/dev/sdb` holds `sdb1` (400 GiB Ceph OSD) + `sdb2` (rest, k3s data-dir).
