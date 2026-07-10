<!-- Copyright (c) 2026 ZyvorAI Labs Private Limited. All rights reserved. -->
# Deployment

Two things get deployed:
1. the **Atlas gateway** (a container on Kubernetes), and
2. optionally, the **Rook Ceph** storage fabric it manages.

This guide covers a k3s node end-to-end, exactly as verified during development.

## Prerequisites on the target host

- A running Kubernetes/k3s cluster; `kubectl` works for your user.
- `podman` (to build the image) and `rsync`, `git`.
- For real Ceph: an **empty block device** for the OSD (destructive to that disk).
- Passwordless `sudo` (for `k3s ctr images import`).

The Atlas gateway image is built **on the host** with podman and imported into k3s containerd —
no external registry, no local Rust toolchain required on your laptop.

## 1. Deploy the gateway (fake Ceph + live k8s driver)

From your workstation:

```bash
./scripts/deploy-remote.sh <host> <user>
# e.g. ./scripts/deploy-remote.sh 212.8.248.187 sus
```

This rsyncs the repo, `podman build`s `atlas-gateway:dev`, imports it into k3s, applies
`deploy/k8s/atlas-gateway.yaml` (Namespace `zyvor-system`, RBAC-scoped ServiceAccount,
Deployment, NodePort **30510**), and verifies `/health` + `/storage-classes`.

RBAC granted (read-only, for the live k8s driver):
`storageclasses`, `volumesnapshotclasses`, `persistentvolumeclaims`, `persistentvolumes`
— `get/list/watch` only.

Verify:

```bash
curl -s http://<host>:30510/health
curl -s http://<host>:30510/api/atlas/v1/storage-classes | jq     # real cluster StorageClasses
curl -s http://<host>:30510/api/atlas/v1/kubernetes/pvs | jq       # real bound PVs
```

The live k8s driver tags Ceph-backed StorageClasses `is_ceph: true`.

## 2. Stand up Rook Ceph (single-node lab)

> **Destructive:** consumes an empty disk (e.g. `/dev/sdb`) as a Ceph OSD.

For a single-node k3s, use the single-node overlay (1 mon, 1 OSD, replica size 1,
failure domain `osd`). Edit `deploy/rook-ceph-lab/single-node/cluster.yaml` to set your **node
name** and **device** first (`kubectl get nodes`, `lsblk`).

```bash
# on the host (or via ssh), from the synced repo dir:
ROOK_BASE=https://raw.githubusercontent.com/rook/rook/release-1.15/deploy/examples
kubectl apply -f deploy/rook-ceph-lab/01-namespaces.yaml
kubectl apply -f "$ROOK_BASE/crds.yaml" -f "$ROOK_BASE/common.yaml" -f "$ROOK_BASE/operator.yaml"
kubectl -n rook-ceph rollout status deploy/rook-ceph-operator --timeout=240s

kubectl apply -f deploy/rook-ceph-lab/single-node/cluster.yaml      # OSD provisioning (a few min)
kubectl apply -f deploy/rook-ceph-lab/single-node/blockpool-sc.yaml # pool + zyvor-rbd-prod SC
kubectl apply -f https://raw.githubusercontent.com/rook/rook/release-1.15/deploy/examples/toolbox.yaml
```

For a real **multi-node** cluster use the canonical manifests instead — `deploy/rook-ceph-lab/up.sh`
(see [that README](../deploy/rook-ceph-lab/README.md)), which also installs KubeVirt/CDI and the
external-snapshotter.

Verify Ceph:

```bash
TB=$(kubectl -n rook-ceph get pod -l app=rook-ceph-tools -o jsonpath='{.items[0].metadata.name}')
kubectl -n rook-ceph exec "$TB" -- ceph -s
kubectl get sc | grep zyvor
```

Prove real RBD provisioning:

```bash
kubectl apply -f - <<'YAML'
apiVersion: v1
kind: PersistentVolumeClaim
metadata: { name: atlas-rbd-smoke, namespace: default }
spec:
  accessModes: ["ReadWriteOnce"]
  storageClassName: zyvor-rbd-prod
  resources: { requests: { storage: 2Gi } }
YAML
kubectl -n default get pvc atlas-rbd-smoke      # → Bound
kubectl -n rook-ceph exec "$TB" -- rbd ls -l rbd-nvme-prod   # → the csi-vol-... image
```

## 3. Run Atlas in real Ceph mode

The real-mode gateway runs in the `rook-ceph` namespace and renders `/etc/ceph` from the Rook
mon Secret via an **initContainer** — the admin credential never leaves the cluster.

```bash
# build the ceph-enabled image (bundles the Reef ceph/rbd client), import, deploy
podman build -t atlas-gateway:ceph -f Dockerfile.ceph .
podman save atlas-gateway:ceph -o /tmp/atlas-ceph.tar && sudo k3s ctr images import /tmp/atlas-ceph.tar
kubectl apply -f deploy/k8s/atlas-gateway.yaml          # ensures the ClusterRole exists
kubectl apply -f deploy/k8s/atlas-gateway-ceph.yaml     # real-mode Deployment on NodePort 30511
kubectl -n rook-ceph rollout status deploy/atlas-gateway-ceph --timeout=150s
```

Verify **real** discovery (Atlas runs `ceph`/`rbd` itself):

```bash
B=http://<host>:30511
curl -s -X POST $B/api/atlas/v1/backends/bkd_ceph_lab/discover | jq
curl -s $B/api/atlas/v1/clusters | jq     # real fsid + capacity
curl -s $B/api/atlas/v1/pools    | jq     # real ceph pools
curl -s $B/api/atlas/v1/volumes  | jq     # real RBD images (incl. the PVC-backed one)
```

A verified run returned the real fsid `5ace73d1-…`, pool `rbd-nvme-prod`, `osd.0` up/in, and the
`csi-vol-…` RBD image created for the `atlas-rbd-smoke` PVC — closing the loop
PVC → CSI → RBD → Atlas discovery.

## Durable database

The gateway's SQLite DB is backed by a **ReadWriteOnce PVC** (not `emptyDir`), so inventory/jobs
survive pod restarts. The Deployments use `strategy: Recreate` because RWO requires the old pod to
release the volume before the new one mounts it.

- `deploy/k8s/atlas-gateway.yaml` → PVC `atlas-gateway-data` on the **cluster default** StorageClass
  (deploys even without Ceph, e.g. k3s `local-path`).
- `deploy/k8s/atlas-gateway-ceph.yaml` → PVC `atlas-gateway-ceph-data` on **`zyvor-rbd-prod`**
  (dogfoods the platform's own Ceph).

Verify durability:
```bash
curl -s http://<host>:30511/api/atlas/v1/volumes | grep -o '"name":"[^"]*"'
kubectl -n rook-ceph rollout restart deploy/atlas-gateway-ceph
kubectl -n rook-ceph rollout status deploy/atlas-gateway-ceph
curl -s http://<host>:30511/api/atlas/v1/volumes | grep -o '"name":"[^"]*"'   # same rows persist
```
For multi-replica HA, switch `ATLAS_DATABASE_URL` to Postgres (the `sqlx` layer abstracts the driver).

## Day-2 durability & health

The control plane survives restarts and rollouts cleanly:

- **Job recovery on boot** — the job engine re-scans `storage_jobs`: a job left `running` when the
  process died is failed-safe (`"interrupted by control-plane restart"`, never stuck), and `queued`
  jobs are re-enqueued. Opt-in bounded retry-with-backoff per job (`max_retries`).
- **Graceful shutdown** — on `SIGTERM` (k8s rollout / `docker stop`) the REST + gRPC servers drain
  in-flight requests before exiting. The Deployment sets `terminationGracePeriodSeconds: 30`.
- **Liveness vs readiness** — `GET /livez` is process-alive (k8s `livenessProbe` → restart);
  `GET /readyz` deep-checks the DB **and probes the backend driver** (no longer a hardcoded `ok`) and
  reports worker heartbeats (k8s `readinessProbe` → depool without killing). Both are wired in
  `deploy/k8s/atlas-gateway.yaml`.
- **Self-state backup** — Atlas can back up its *own* SQLite state (inventory, jobs, audit, quotas,
  DataBridge plans) to S3/RGW via `VACUUM INTO` snapshots. Disabled unless configured:

  | Env | Default | Meaning |
  |---|---|---|
  | `ATLAS_STATE_BACKUP_SECS` | `0` (off) | snapshot interval; `0` disables |
  | `ATLAS_STATE_BACKUP_ENDPOINT` / `_BUCKET` | — | S3/RGW endpoint + bucket (required to enable) |
  | `ATLAS_STATE_BACKUP_ACCESS_KEY` / `_SECRET_KEY` | — | S3 credentials |
  | `ATLAS_STATE_BACKUP_REGION` | `us-east-1` | S3 region |
  | `ATLAS_STATE_BACKUP_PREFIX` | `atlas-state` | key prefix |
  | `ATLAS_STATE_BACKUP_KEEP` | `24` | snapshots retained (older pruned) |

## Image name note

`podman save` preserves the `localhost/` prefix, so k3s imports the image as
`localhost/atlas-gateway:dev` (and `:ceph`). The Deployments reference those names with
`imagePullPolicy: Never`. If you push to a registry instead, update the `image:` fields.

## Teardown

```bash
kubectl delete -f deploy/k8s/atlas-gateway-ceph.yaml --ignore-not-found
kubectl delete -f deploy/k8s/atlas-gateway.yaml --ignore-not-found
kubectl -n default delete pvc atlas-rbd-smoke --ignore-not-found
# Ceph (this releases the OSD disk):
kubectl -n rook-ceph delete cephcluster rook-ceph
# then follow Rook cleanup docs to wipe /var/lib/rook and the OSD disk before reuse.
```
