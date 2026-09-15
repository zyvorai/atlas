<!-- Copyright (c) 2026 ZyvorAI Labs Private Limited. -->
<!-- SPDX-License-Identifier: AGPL-3.0-only OR LicenseRef-Atlas-Commercial -->
# Atlas Helm chart

Packages what `deploy/k8s/atlas-gateway.yaml` (fake/k8s driver) and
`deploy/k8s/atlas-gateway-ceph.yaml` (real Ceph driver) apply by hand into one versioned,
upgradeable chart. See `docs/DEPLOYMENT.md` for the full deployment picture (this chart replaces
the "apply these YAML files directly" step, not the Rook/Ceph prerequisites).

## Quickstart (fake driver, no Ceph needed)

```bash
helm install atlas ./deploy/helm/atlas \
  --create-namespace \
  --set auth.createSecret=true
```

This generates a random JWT secret and admin password in a Secret the chart creates — fine for a
local/lab install, but read `values.yaml`'s `auth.createSecret` comment before doing this for
anything beyond that: the generated values land in `helm get values` and Helm's release history.
For a real deployment, create the `atlas-gateway-auth` Secret yourself first (see
`deploy/k8s/atlas-auth-secret.example.yaml` or `deploy/vault-lab/` for an externally-managed
alternative) and leave `auth.createSecret` at its default `false`.

## Real Ceph deployment

```bash
helm install atlas-ceph ./deploy/helm/atlas \
  --set ceph.enabled=true \
  --set image.repository=localhost/atlas-gateway \
  --set image.tag=ceph \
  --set service.grpc.enabled=true
```

Expects to run in `ceph.rookNamespace` (default `rook-ceph`) with Rook's mon endpoints ConfigMap
and admin keyring Secret already present — this chart doesn't stand up Ceph itself, see
`deploy/rook-ceph-lab/`.

## What's parameterized

See `values.yaml` for the full set — image, replicas/resources, database backend, service
type/ports, ingress, PVC size/class, OIDC/SSO, self-state backup, native alerting sinks
(webhook/PagerDuty/Slack/Opsgenie — `docs/ALERTING.md`), the secrets backend (env vars or Vault —
`docs/SECRETS.md`), OpenTelemetry tracing (`docs/TRACING.md`), and the NFS/ZFS demo backends.

## Multi-replica (Postgres-backed) deployment

By default (`database.kind: sqlite`) Atlas is single-replica: the DB lives on a `ReadWriteOnce`
PVC only one pod can mount, so `templates/deployment.yaml` uses a `Recreate` rollout regardless of
`replicaCount`. The query layer itself is backend-agnostic (`sqlx::AnyPool` — see `docs/HA.md`),
so switching is a values change, not a code change:

```bash
helm install atlas ./deploy/helm/atlas \
  --set database.kind=postgres \
  --set database.existingSecret=atlas-postgres-auth \
  --set replicaCount=3
```

`database.existingSecret` must hold the full `postgres://user:pass@host:port/db` connection string
under `database.secretKey` (default `database-url`) — this chart does not stand up Postgres
itself; point it at a real managed/HA instance (RDS, CloudNativePG, Patroni-managed) or
`deploy/postgres-lab/` for a throwaway lab one. With `database.kind: postgres` the PVC is not
rendered at all and the rollout strategy becomes `RollingUpdate`.

## Not parameterized (yet)

DB-backed rate limiting: `rateLimitRpm` is still enforced per-pod (an in-process `HashMap`), so
each replica gets its own independent window rather than sharing one cluster-wide budget across
`replicaCount` pods — see `docs/HA.md`'s remaining items.

## Verifying a render before installing

```bash
helm lint ./deploy/helm/atlas
helm template atlas ./deploy/helm/atlas --set ceph.enabled=true | kubectl apply --dry-run=client -f -
```
