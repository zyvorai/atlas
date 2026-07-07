<!-- Copyright (c) 2026 ZyvorAI Labs Private Limited. All rights reserved. -->
# Single-node Rook Ceph overlay

The top-level `deploy/rook-ceph-lab/` manifests assume a 3-node cluster (replica size 3, host
failure domain). This overlay makes Rook Ceph run on **one node** (k3s dev box): replica size 1,
`failureDomain: osd`, single MDS/mon. Use it for the lab on `212.8.248.187`, not for production.

Apply it **after** the operator + CephCluster are up (steps 1–4 of the parent `up.sh`), replacing the
parent's pool/filesystem/RGW manifests with these single-node variants:

```sh
cd deploy/rook-ceph-lab/single-node
kubectl apply -f cluster.yaml        # CephCluster: 1 mon, useAllDevices, osd failure domain
kubectl apply -f blockpool-sc.yaml   # RBD pool (size 1) + zyvor-rbd-prod StorageClass (RWO block)
kubectl apply -f cephfs-sc.yaml      # CephFS (size 1) + zyvor-cephfs-shared StorageClass (RWX file)
kubectl apply -f rgw.yaml            # CephObjectStore + zyvor-rgw-bucket StorageClass (S3/OBC)
```

Wait for each to settle:

```sh
kubectl -n rook-ceph get cephcluster            # PHASE Ready, HEALTH_WARN is expected (1 OSD < size 3)
kubectl -n rook-ceph get cephfilesystem zyvorfs # PHASE Ready (2 MDS pods)
kubectl -n rook-ceph get cephobjectstore        # PHASE Ready (RGW pod)
kubectl get sc                                  # zyvor-rbd-prod, zyvor-cephfs-shared, zyvor-rgw-bucket
```

## StorageClasses this overlay creates

| StorageClass          | Backend | Access | Used by Atlas policy |
|-----------------------|---------|--------|----------------------|
| `zyvor-rbd-prod`      | RBD     | RWO    | `production`, `database`, `development`, `ai` |
| `zyvor-cephfs-shared` | CephFS  | RWX    | `shared` (ISO libraries, templates, multi-writer product data) |
| `zyvor-rgw-bucket`    | RGW     | S3     | buckets / backups (ObjectBucketClaim) |

`POST /volumes {"policy":"shared", ...}` provisions a **ReadWriteMany** CephFS volume that multiple
pods can mount at once (verified: two pods writing the same volume concurrently). RBD volumes are
RWO — a single writer.
