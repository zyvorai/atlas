# Dex OIDC lab

Throwaway [Dex](https://dexidp.io/) identity provider for testing Atlas's OIDC/SSO login
(`crates/atlas-gateway/src/routes/oidc.rs`, `ATLAS_OIDC_*` env vars). Plain HTTP, in-memory
storage, static test users — **for the lab, not for production**, same posture as
`deploy/rook-ceph-lab/`.

## Install

```
./up.sh
```

Idempotent — rerun any time; the OAuth2 client secret is generated once (stored in the
`dex-oidc-client` Secret) and reused on subsequent runs, so redeploying Dex doesn't invalidate an
already-configured Atlas gateway.

## What you get

| Object | Purpose |
|---|---|
| `Namespace zyvor-system` | created if missing (same namespace as the fake/k8s `atlas-gateway`) |
| `ConfigMap dex-config` | rendered `config.yaml` — issuer, static client, static test users |
| `Secret dex-oidc-client` | the generated OAuth2 client secret shared with Atlas |
| `Deployment/Service dex` | single replica, NodePort 30556 |

Static test users (email / password / group → Atlas role):

| Email | Password | Dex group | Atlas role |
|---|---|---|---|
| alice@zyvor.lab | `AlicePass123!` | `admin` | admin |
| bob@zyvor.lab | `BobPass123!` | `operator` | operator |
| carol@zyvor.lab | `CarolPass123!` | *(none)* | viewer (default) |

## Wire it into Atlas

`up.sh` prints the exact values at the end. Set them as env vars on the `atlas-gateway`
Deployment (namespace `zyvor-system`) and redeploy:

```
ATLAS_OIDC_ISSUER_URL=http://<node-ip>:30556
ATLAS_OIDC_CLIENT_ID=atlas-console
ATLAS_OIDC_CLIENT_SECRET=<printed by up.sh>
ATLAS_OIDC_REDIRECT_URL=http://<node-ip>:30510/api/atlas/v1/auth/oidc/callback
ATLAS_OIDC_ADMIN_GROUP=admin
ATLAS_OIDC_OPERATOR_GROUP=operator
```

## Verify

```
curl -s http://<node-ip>:30556/.well-known/openid-configuration | head
curl -s http://<node-ip>:30510/api/atlas/v1/auth/oidc/status   # {"enabled":true} once Atlas has the env set
```

Then open `http://<node-ip>:30510/`, click **Sign in with SSO**, and log in as one of the test
users above.

## Teardown

```
kubectl -n zyvor-system delete deploy/dex svc/dex configmap/dex-config secret/dex-oidc-client
```

(No `teardown.sh` — this is a 4-object deploy, not worth the ceremony `deploy/rook-ceph-lab/`
needs for a whole Ceph cluster.)
