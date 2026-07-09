<!-- Copyright (c) 2026 ZyvorAI Labs Private Limited. All rights reserved. -->
# Zyvor DataBridge — cloud-to-edge database mobility

DataBridge migrates **managed cloud databases** (AWS RDS/Aurora, GCP Cloud SQL — PostgreSQL & MySQL)
to **open, self-managed engines at the edge** on Kubernetes, with **Ceph (via Atlas/Rook) as the
storage layer**. It runs from Atlas as a migration control plane, reusing the job engine, inventory,
gateway, and Zeus OS console. Ceph is the *storage*, not the database engine.

> Positioning: *"Migrate managed cloud databases to open engines running on Zyvor Edge, backed by
> Ceph — with continuous replication, validation, cutover, and rollback from one control plane."*
> DynamoDB / Firestore / Spanner are **out of scope** — those are data-model migrations, not storage
> migrations.

## Pipeline
```
Discover → Assess → Provision (edge DB on Ceph) → Full-load → CDC (Debezium)
        → Validate → Cutover → (Rollback within window)
```
Each stage is an async job (`202 + job id`, progress via `/jobs/{id}/watch`). Long-running work
(edge CR readiness, CDC lag) is advanced by the **DataBridge reconciler** worker.

- **Edge runtime**: CloudNativePG (Postgres) / Percona XtraDB (MySQL), data + WAL on the
  `zyvor-rbd-prod` Ceph RBD StorageClass.
- **CDC**: Debezium on Strimzi/Kafka (cloud-neutral, reads WAL/binlog).
- **Cutover** is admin-guarded: plan `validated` + last validation `passed` + CDC `lag_seconds` under
  threshold; **rollback** only within the plan's rollback window.

## Fake vs real
The whole pipeline runs **fake-first** with no cloud creds or operators — connectors serve a canned
schema, edge provisioning/full-load/CDC/validate complete inline, and the reconciler drains a
synthetic CDC lag so the UI animates. Set a source's `driver_mode: real` (creds in a k8s Secret) and
install the edge operators to run it for real.

## Run locally (fake, no cloud/k8s)
```bash
make run-databridge          # gateway with fake Ceph driver + DataBridge reconciler @5s
```
Then open the console at http://127.0.0.1:5110/ → **DataBridge → Cloud Databases**, register a
source (kind `postgres`/`mysql`, driver mode `fake`), and walk the plan through the stepper. Or over
REST:
```bash
B=http://127.0.0.1:5110/api/atlas/v1
SID=$(curl -sX POST $B/databridge/sources -d '{"name":"orders","kind":"postgres","cloud":"rds"}' | jq -r .id)
curl -sX POST $B/databridge/sources/$SID/discover
PID=$(curl -sX POST $B/databridge/plans -d "{\"name\":\"orders\",\"source_id\":\"$SID\"}" | jq -r .id)
for stage in assess provision full-load cdc/start validate cutover; do curl -sX POST $B/databridge/plans/$PID/$stage; done
```

## Run for real (edge cluster)
1. Install the edge operators: [`deploy/databridge/up.sh`](../deploy/databridge/README.md)
   (CloudNativePG + Percona + Strimzi) — needs the `zyvor-rbd-prod` StorageClass.
2. Register a source with `driver_mode: real` and `secret_ref`/`secret_namespace` pointing at a k8s
   Secret holding the source credentials (never sent in the API).
3. Run the pipeline; the edge `Cluster` CR binds PVCs on Ceph, the reconciler polls it to `ready`.

## REST endpoints (`/api/atlas/v1`)
| Method | Path | Purpose |
|---|---|---|
| GET/POST | `/databridge/sources` | list / register sources |
| GET/DELETE | `/databridge/sources/{id}` | get / delete a source |
| POST | `/databridge/sources/{id}/discover` | discover schema (job) |
| GET/POST | `/databridge/plans` | list / create plans |
| GET | `/databridge/plans/{id}` | plan detail |
| POST | `/databridge/plans/{id}/assess` | score readiness (job) |
| POST | `/databridge/plans/{id}/provision` | provision edge DB (job) |
| POST | `/databridge/plans/{id}/full-load` | full-load (job) |
| POST | `/databridge/plans/{id}/cdc/start` · `/cdc/stop` | CDC control (job) |
| POST | `/databridge/plans/{id}/validate` | validate source vs edge (job) |
| POST | `/databridge/plans/{id}/cutover` | cutover — **admin, guarded** (job) |
| POST | `/databridge/plans/{id}/rollback` | rollback within window — **admin** (job) |
| GET | `/databridge/{edge-clusters,cdc-streams,validations,cutovers}` | read models |

## Configuration
| Env | Default | Meaning |
|---|---|---|
| `ATLAS_DATABRIDGE_RECONCILE_SECS` | `15` | reconciler tick (edge CR readiness, CDC lag). `0` disables. |

Source `driver_mode` (`fake`/`real`) is per-source, set at registration. Edge namespace is
`zyvor-databridge`; edge storage class is `zyvor-rbd-prod`.

## Data model
`migration_sources`, `migration_plans`, `edge_db_clusters`, `cdc_streams`, `validation_runs`,
`cutovers` (SQLite; migration `0011_databridge.sql`). Plan state machine:
`draft → discovered → assessed → provisioning → provisioned → full_loading → loaded →
cdc_streaming → validating → validated → cutover_pending → cutover_in_progress → cutover_complete
→ completed | rolled_back | failed`.

## Live verification (real infra)
**A full end-to-end real migration was driven through the deployed gateway** on the Rook cluster
(k3s + Rook Ceph), Postgres → Ceph-backed edge Postgres:

| Stage | Result |
|---|---|
| discover | gateway's `tokio-postgres` connector introspected a live Postgres **16.14** (tables, PKs, sizes, `wal_level=logical`) |
| assess | readiness **90/100** (flagged the no-PK `audit_log`) |
| provision | gateway applied a **CloudNativePG `Cluster` CR**; data + WAL PVCs **Bound on `zyvor-rbd-prod`**; reconciler advanced `provisioning → provisioned` when it reached healthy |
| full-load | gateway ran a **`pg_dump\|psql` batch Job**; **data really copied** (100 customers, 500 orders verified in the edge DB) |
| validate | gateway ran a **row-count-compare Job**; reconciler → `validated` |
| cutover | guarded (validated + validation passed) → endpoint switched source → `edge-…-rw.zyvor-databridge.svc:5432`, 72h rollback window |
| rollback | within window → `rolled_back` |

Prereqs proven in the process: `deploy/databridge/up.sh --pg-only` installs CloudNativePG on the real
cluster; the gateway's **write RBAC** (CNPG `clusters`, batch `jobs`, Strimzi connectors, `secrets`)
is sufficient; the CNPG `<cluster>-app` Secret exposes the `uri`/`password` keys the loader uses.

**CDC (Strimzi) — partially exercised live:** installed the Strimzi operator + brought up a KRaft
Kafka on the cluster, which corrected three real bugs in the CDC CR builders (now fixed + validated
by server-side dry-run against the real CRDs):
1. Strimzi serves the kinds at `kafka.strimzi.io/**v1**` (not `v1beta2`);
2. the bootstrap Service is `<kafka>-**kafka**-bootstrap:9092`;
3. `KafkaConnect` requires top-level `groupId` + `configStorageTopic`/`offsetStorageTopic`/
   `statusStorageTopic` (not inside `config`).

**Remaining for full CDC data-flow**: a Kafka Connect image that **bundles the Debezium + a JDBC-sink
plugin** (the stock Strimzi Connect image has none) — set `ATLAS_DATABRIDGE_CONNECT_IMAGE` to a
prebuilt/registry image; plus Debezium config iteration (replication slot/publication) and real CDC
**lag tracking** (Kafka Connect offsets). This is a dedicated build/registry follow-up.

## Status
- **Done + CI-tested**: full pipeline demoable end-to-end (fake) — locked by
  `tests/databridge_pipeline.rs`; real edge provisioning (CNPG/Percona CR apply + reconciler readiness
  polling) on Ceph; **real full-load** (`pg_dump|psql` / `mysqldump|mysql` batch Jobs, reconciler-watched).
- **Done, UNVERIFIED against a live stack** (unit-tested CR/Job builders; need a running Strimzi/Kafka
  + real DB to prove end-to-end):
  - real **CDC** — `start_cdc` applies a Strimzi `KafkaConnect` cluster + a Debezium source
    `KafkaConnector` + a JDBC-sink `KafkaConnector` (creds via `${secrets:…}` config-provider);
  - real **validation** — `validate` applies a row-count-compare batch Job (source↔edge), reconciler
    records passed/failed.
- **Follow-up (single remaining)**: real CDC **lag tracking** (Kafka Connect consumer-group offsets /
  source LSN) — the reconciler only synthesizes lag in fake mode today, so the lag-gated cutover
  guard needs real lag before a real cutover is safe.

### Full-load secret assumptions
The real full-load Job runs in `zyvor-databridge`, so the **source Secret must exist in that
namespace** with `username`/`password` keys. The edge Secret is operator-generated: CloudNativePG's
`<cluster>-app` provides a ready-to-use `uri`; Percona's `<cluster>-secrets` provides `root`.
