<!-- Copyright (c) 2026 ZyvorAI Labs Private Limited. -->
<!-- SPDX-License-Identifier: AGPL-3.0-only OR LicenseRef-Atlas-Commercial -->
# postgres-lab

Throwaway Postgres for verifying atlas-inventory's Phase-1 HA scaffolding
(`connect_postgres`/`migrate_postgres`, see `docs/HA.md`) against real infrastructure, not just a
compile-check behind the `postgres` cargo feature.

```
./up.sh
```

Stands up, in the lab k3s cluster's `zyvor-system` namespace:
- A generated-password Secret (`postgres-lab-auth`), never committed or printed by the script
- `postgres:16-alpine`, single replica, ephemeral `local-path` PVC
- A NodePort Service on `30432`

Then run the live verification test:

```
DATABASE_URL="$(kubectl -n zyvor-system get secret postgres-lab-auth -o jsonpath='{.data.database-url}' | base64 -d | sed 's#postgres-lab.zyvor-system.svc:5432#<NODE_IP>:30432#')"
cargo test -p atlas-inventory --features postgres --test postgres_live -- --ignored --nocapture
```

## What this proves, and what it doesn't

Proves: `connect_postgres()` opens a real Postgres connection and `migrate_postgres()` runs
`migrations-postgres/` clean against it — the connection/schema scaffolding described in
`docs/HA.md` actually works, not just compiles.

Does **not** prove: that Atlas can run its query layer against Postgres. `atlas-inventory`'s
read/write query paths are still SQLite-only (`SqlitePool` used throughout); porting them to be
backend-agnostic is a separate, explicitly out-of-scope effort tracked in `docs/HA.md`. A real HA
deployment also needs replication/failover (e.g. Patroni, CloudNativePG) — this single-replica lab
target is a schema-verification smoke test, not an HA reference topology.

## Adopting this for real

A bank pilot would point `DATABASE_URL` at their own managed/HA Postgres (RDS, CloudNativePG,
Patroni-managed, etc.) instead of this lab container — the `connect_postgres`/`migrate_postgres`
code path doesn't change, only the connection string and the operational guarantees behind it.

## Teardown

```
kubectl -n zyvor-system delete deploy/postgres-lab svc/postgres-lab pvc/postgres-lab-data secret/postgres-lab-auth
```
