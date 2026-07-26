<!-- Copyright (c) 2026 ZyvorAI Labs Private Limited. All rights reserved. -->
# Cross-cluster DR (RBD mirroring)

Atlas exposes a control-plane catalog and failover API for Ceph RBD mirroring. The real
`rbd mirror` CLI paths run as jobs and still need a **live second Ceph cluster** to be
production-verified. Fake mode (`ATLAS_CEPH_DRIVER_MODE=fake` / `make run`) exercises the full
API and catalog without calling `rbd`.

## Status

| Layer | State |
|---|---|
| Peers / mirrors catalog | Done |
| Enable / disable / promote / demote API + jobs | Done |
| Role transition guards + force promote | Done |
| Preflight + one-click failover runbook | Done |
| Fake-mode job success (no second cluster) | Done |
| Live two-site `rbd mirror` verification | **Pending** (needs peer cluster) |

## API

| Method | Path | Notes |
|---|---|---|
| `POST` | `/dr/peers` | Register peer (`secret_ref` = k8s Secret name, never the token) |
| `GET` | `/dr/peers` | List peers |
| `DELETE` | `/dr/peers/{id}` | Remove peer (+ dependent mirrors) |
| `POST` | `/volumes/{id}/mirror?mode=snapshot&peer=` | Enable (requires a registered peer) |
| `DELETE` | `/volumes/{id}/mirror` | Disable |
| `GET` | `/dr/mirrors` · `/dr/status` | Catalog + posture (`verified: false` until live) |
| `GET` | `/dr/preflight` | Checklist before failover |
| `POST` | `/dr/mirrors/{id}/demote` | Primary → secondary |
| `POST` | `/dr/mirrors/{id}/promote?force=0\|1` | Secondary → primary (`force` = split-brain) |
| `POST` | `/dr/failover` | `{ mirror_id, confirm: true, force? }` runbook |
| `POST` | `/dr/mirrors/{id}/rpo` | `{ rpo_seconds }` observed RPO |

Guards: promote of an already-primary mirror is **409** unless `?force=1`; demote of an already-secondary
is **409**; disabled mirrors cannot be promoted/demoted.

## Failover drill (fake)

```bash
make run
B=http://127.0.0.1:5110/api/atlas/v1

# Peer + direct-RBD volume (seed via API or SQL in tests)
curl -sS -X POST $B/dr/peers -H 'Content-Type: application/json' \
  -d '{"name":"dc2","cluster_fsid":"fsid-2","secret_ref":"dc2-bootstrap"}'

curl -sS $B/dr/preflight | jq
curl -sS -X POST $B/dr/failover -H 'Content-Type: application/json' \
  -d '{"mirror_id":"<id>","confirm":true}'
```

## Live two-site checklist (when a second cluster exists)

1. Bootstrap RBD mirroring between sites (`rbd mirror pool peer bootstrap` / Rook CephRBDMirror).
2. Store the peer bootstrap token in a k8s Secret; register the peer with `secret_ref`.
3. Enable mirroring on critical volumes (`mode=snapshot` or `journal`).
4. Confirm `GET /dr/preflight` is ready; run a scheduled demote/promote drill.
5. Measure RPO and `POST /dr/mirrors/{id}/rpo`.
6. Document site roles and force-promote policy for split-brain.

Until step 1–6 are done on real hardware, treat DR as **control-plane complete, data-plane unverified**.
