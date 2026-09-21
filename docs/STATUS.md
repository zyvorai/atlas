<!-- Copyright (c) 2026 ZyvorAI Labs Private Limited. -->
<!-- SPDX-License-Identifier: LicenseRef-Zyvor-Production-1.0 -->
# Status

Short maturity matrix for Atlas 0.4.0. History and narrative live in [ROADMAP.md](ROADMAP.md).
When the two disagree, this file wins.

A cell is **yes** only for that column. Lab verification is not production support. Nothing
below is bank or enterprise GA.

| Capability | Implemented and unit-tested | Verified on real infrastructure | Production-supported | Experimental | Planned |
|---|---|---|---|---|---|
| Ceph / Rook control plane (RBD, CephFS, RGW, jobs) | yes | yes, single-node Rook lab | no | | |
| NFS and ZFS drivers | yes, fake and real modes | no remote production target | no | real mode | SAN, cloud block, external Ceph import |
| SQLite default, Postgres query layer, Helm `database.kind`, cross-replica rate limiting | yes | yes, CI `postgres-test` and `deploy/postgres-lab/` | no — needs a real HA Postgres | | enterprise IdP for OIDC (Dex lab only) |
| Auth, tenants, quotas, audit export, Vault resolution | yes | yes, lab (Dex, Vault, SIEM receiver) | no | | |
| DataBridge Postgres | yes | yes, through cutover | no | TLS still `sslmode=disable` | verified TLS migration |
| DataBridge MariaDB | yes | yes, through cutover | no | | repeatable CI/lab automation |
| DataBridge MongoDB | yes | yes, through cutover | no | | change-stream edge cases, repeatable verification |
| DataBridge MySQL | yes | CDC live for `DATETIME` only | no | `TIMESTAMP` CDC | cutover; `TIMESTAMP` SMT |
| DataBridge SQL Server and Oracle | discovery yes | discovery live | no | | full-load, CDC, validate, cutover, rollback; Oracle TCPS |
| Cross-cluster DR (RBD mirror) | control plane yes; `dataplane_verified` is false | no | no | yes | two-site promote/demote drill |
| Ops Advisor, incidents, what-if, anomalies, MCP | yes, read-only | console exercised on the lab gateway | no | | persisted findings; no execution |
| Product integrations beyond gRPC `Owner` | gRPC owner surface yes | | no | | Transiva import, Veyron, GuestKit, PacketWolf, then a small SDK |

Transiva's owner id on the wire remains `hyper2kvm`. v0.4.0 does not rename it.

**Unresolved:** Zeus OS already has an `atlas` module ("Machine Finder") and a Storage Center
UI. Atlas does not yet absorb, replace, or sit beside that UI. Do not treat the names as settled.
