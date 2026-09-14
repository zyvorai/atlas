<!-- Copyright (c) 2026 ZyvorAI Labs Private Limited. -->
<!-- SPDX-License-Identifier: AGPL-3.0-only OR LicenseRef-Atlas-Commercial -->
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
| Preflight + one-click failover runbook | Done (`control_plane_ready`; `dataplane_verified` now a config flag — see below) |
| Fake-mode job success (no second cluster) | Done (`cargo test -p atlas-gateway --test dr`) |
| Live two-site `rbd mirror` peer bootstrap | **Attempted, not completed** (2026-08-25 — see below) |
| Live promote/demote drill through Atlas's API | **Pending** (blocked on the above) |

`GET /dr/status` and `GET /dr/preflight` both expose `control_plane_ready` (catalog coherent) and
`dataplane_verified`. The latter used to be a hard-coded `false`; it's now `Config::
dr_dataplane_verified` (env `ATLAS_DR_DATAPLANE_VERIFIED`, default `false`) — a per-deployment
toggle an operator sets only after *personally* completing the checklist below against their own
real hardware, never a blanket product claim. Preflight `ready` means you can enqueue a failover
**job**; it is not a claim that Ceph mirroring is live.

### 2026-08-25 real two-cluster attempt — what was reached, what blocked it

Using the two real Rook Ceph clusters already in this lab (`<ephemeral-ip>`, fsid
`18675a0d-6bd6-455a-aca6-3a045d79a46f`, and `<ephemeral-ip>`, fsid
`e51cf24f-f89f-4061-8635-6e07caa8a3f9` — confirmed distinct), not a synthetic second cluster:

- Created a dedicated `atlas-dr-mirror-test` CephBlockPool (image mirroring mode) on both clusters
  — deliberately isolated from any pool carrying real tenant data.
- Deployed a `CephRBDMirror` daemon (`atlas-dr-mirror`) on both clusters via Rook; both came up
  `Running` (slowly — see below).
- Generated a real `rbd mirror pool peer bootstrap create` token on the primary. Discovered the
  token embeds the mon's address as a Kubernetes **ClusterIP** (`10.43.12.84`), which is not
  routable from the peer cluster — a real architectural gap for any two genuinely separate
  clusters, not specific to this lab. Worked around it by NodePort-exposing `rook-ceph-mon-a`
  (alongside its existing ClusterIP, non-destructively) and rewriting the token's embedded
  `mon_host` to the externally-reachable `host:nodePort` pair.
- Copied the corrected token to the secondary cluster and got as far as `rbd mirror pool peer
  bootstrap import` actually **reaching** the local mon (connection succeeded) before failing:
  `(13) Permission denied` on `site_name_set`. The `rbd-mirror` daemon's own cephx identity
  (`client.rbd-mirror.a`) is deliberately scoped and lacks the mon-config capability needed to set
  a cluster's mirror site name — that operation needs `client.admin`-equivalent privilege, which
  this session stopped short of extracting from the live cluster's Secret rather than pull a
  full cluster-admin credential just to finish a lab drill.
- **Rolled back the NodePort exposure** on the primary's mon (back to ClusterIP-only) since the
  peer relationship was never completed and there was no reason to leave it reachable. Left the
  test pool and mirror daemon deployed on both clusters (harmless, clearly named, isolated from
  real data) as an honest record of how far this got and a head start for whoever finishes it.

**To actually finish this**: run the same `bootstrap import` step with `client.admin` (or a
purpose-built cephx identity granted `mon 'allow *'` scoped just for this), then `rbd mirror pool
enable atlas-dr-mirror-test image` on both sides, create a real image, `rbd mirror image enable
<pool>/<image> snapshot`, and drive the promote/demote cycle through Atlas's `/dr/*` API as
originally planned below. Only then set `ATLAS_DR_DATAPLANE_VERIFIED=true` on the deployment that
actually completed it.

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
   **Gotcha (confirmed 2026-08-25):** the bootstrap token embeds the mon's address as whatever
   `mon_host` the local cluster resolves to — inside Kubernetes that's a ClusterIP, not routable
   from a genuinely separate cluster. Either NodePort/LoadBalancer-expose the mon (Rook won't do
   this for you) and rewrite the token's `mon_host` to the external address before importing it
   on the peer, or run the bootstrap from outside Kubernetes against a routable mon endpoint.
   **Gotcha:** `rbd mirror pool peer bootstrap import` needs `client.admin`-equivalent mon
   capability (it sets the cluster's mirror site name) — the `rbd-mirror` daemon's own scoped
   cephx identity (`client.rbd-mirror.<id>`) does not have this and will fail with `(13)
   Permission denied` on `site_name_set`. Use `client.admin` (from the `rook-ceph-mon` Secret) or
   a purpose-built identity with `mon 'allow *'` for this one step only.
2. Store the peer bootstrap token in a k8s Secret; register the peer with `secret_ref`.
3. Enable mirroring on critical volumes (`mode=snapshot` or `journal`).
4. Confirm `GET /dr/preflight` is ready; run a scheduled demote/promote drill.
5. Measure RPO and `POST /dr/mirrors/{id}/rpo`.
6. Document site roles and force-promote policy for split-brain.

Until step 1–6 are done on real hardware, treat DR as **control-plane complete, data-plane
unverified** — and even then, only for the specific deployment that did it (see
`Config::dr_dataplane_verified` above).
