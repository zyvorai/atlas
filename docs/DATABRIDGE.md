<!-- Copyright (c) 2026 ZyvorAI Labs Private Limited. All rights reserved. -->
# Zyvor DataBridge — cloud-to-edge database mobility

DataBridge migrates **managed cloud databases** to **open, self-managed engines at the edge** on
Kubernetes, with **Ceph (via Atlas/Rook) as the storage layer**. It runs from Atlas as a migration
control plane, reusing the job engine, inventory, gateway, and Zeus OS console. Ceph is the *storage*,
not the database engine.

**Supported source engines** (`kind`):

| Source engine | Aliases | Edge target | Migration | Real connector |
|---|---|---|---|---|
| PostgreSQL | `postgres`, `postgresql`, `pg` | CloudNativePG (Postgres) | homogeneous | `tokio-postgres` (default) |
| MySQL | `mysql` | Percona XtraDB (MySQL) | homogeneous | `sqlx` (default) |
| MariaDB | `mariadb`, `maria` | Percona XtraDB (MySQL) | homogeneous | `sqlx` (default) |
| Oracle | `oracle`, `ora` | CloudNativePG (Postgres) | **heterogeneous** | `oracle`/OCI (feature `oracle`) |
| SQL Server | `sqlserver`, `mssql`, `sql-server` | CloudNativePG (Postgres) | **heterogeneous** | `tiberius` (feature `sqlserver`) |
| MongoDB | `mongodb`, `mongo` | Percona Server for MongoDB (**document**) | homogeneous | `mongodb` driver (feature `mongodb`) |

*Homogeneous* migrations (Postgres/MySQL/MariaDB/MongoDB) copy the data with a `dump→restore` full-load
Job, then Debezium streams changes (`snapshot.mode=never`). *Heterogeneous* migrations (Oracle / SQL
Server → Postgres) have **no dump full-load**: Debezium's `initial` snapshot seeds the edge and the JDBC
sink `auto.create`s the tables, then it streams (the standard cross-engine pattern). **MongoDB** is the
one document engine — it lands on a Percona Server for MongoDB replica set (`rs0`, required for change
streams), full-loads with `mongodump | mongorestore`, and uses the **MongoDB Kafka sink** (with the
Debezium Mongo CDC handler) instead of the JDBC sink. Every engine runs the whole pipeline
**fake-first** with no cloud creds; the real Oracle / SQL Server / MongoDB connectors are behind cargo
features because they link a native client (OCI) / TLS stack / driver.

> Positioning: *"Migrate managed cloud databases (RDS/Aurora, Cloud SQL, Azure SQL, on-prem Oracle)
> to open engines running on Zyvor Edge, backed by Ceph — with continuous replication, validation,
> cutover, and rollback from one control plane."*
> DynamoDB / Firestore / Spanner are **out of scope** — those are data-model migrations, not storage
> migrations.

## Pipeline
```
Discover → Assess → Provision (edge DB on Ceph) → Full-load → CDC (Debezium)
        → Validate → Cutover → (Rollback within window)
```
Each stage is an async job (`202 + job id`, progress via `/jobs/{id}/watch`). Long-running work
(edge CR readiness, CDC lag) is advanced by the **DataBridge reconciler** worker.

- **Edge runtime**: CloudNativePG (Postgres) / Percona XtraDB (MySQL) / Percona Server for MongoDB,
  data + WAL on the `zyvor-rbd-prod` Ceph RBD StorageClass.
- **CDC**: Debezium on Strimzi/Kafka (cloud-neutral, reads WAL/binlog/redo/oplog — Postgres, MySQL,
  MariaDB, Oracle LogMiner, SQL Server, MongoDB change streams) → Aiven JDBC sink (relational) or the
  MongoDB Kafka sink (document) → edge DB.
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
source (kind `postgres`/`mysql`/`mariadb`/`oracle`/`sqlserver`, driver mode `fake`), and walk the plan
through the stepper. Or over REST:
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
| POST | `/databridge/plans/{id}/cdc/start` · `/cdc/stop` · `/cdc/restart` | CDC control + self-heal (job) |
| POST | `/databridge/plans/{id}/validate` | validate source vs edge (job) |
| POST | `/databridge/plans/{id}/cutover` | cutover — **admin, guarded** (job) |
| POST | `/databridge/plans/{id}/rollback` | rollback within window — **admin** (job) |
| GET | `/databridge/{edge-clusters,cdc-streams,validations,cutovers}` | read models |

## Configuration
| Env | Default | Meaning |
|---|---|---|
| `ATLAS_DATABRIDGE_RECONCILE_SECS` | `15` | reconciler tick (edge CR readiness, full-load/validation Jobs, CDC health). `0` disables. |
| `ATLAS_DATABRIDGE_CONNECT_IMAGE` | *(unset)* | Kafka Connect image bundling Debezium + a JDBC-sink plugin (see `deploy/databridge/connect/Dockerfile`). Required for real CDC. |
| `ATLAS_DATABRIDGE_SINK_PK_FIELDS` | `id` | record-key PK column(s) the JDBC sink upserts on. |

Source `driver_mode` (`fake`/`real`) is per-source, set at registration. Edge namespace is
`zyvor-databridge`; edge storage class is `zyvor-rbd-prod`. Real CDC also expects a Strimzi Kafka named
`zyvor-kafka` in the edge namespace (bootstrap `zyvor-kafka-kafka-bootstrap:9092`).

### Cargo features (`atlas-databridge`)
The default build supports **fake mode for all engines** and **real Postgres / MySQL / MariaDB** (no
extra system libraries). The real connectors that need a native dependency, and precise CDC lag, are
behind features so a missing lib never breaks the default build:

| Feature | Enables | Native dependency |
|---|---|---|
| *(default)* | fake all engines + real Postgres (`tokio-postgres`), MySQL/MariaDB (`sqlx`) | none |
| `sqlserver` | real SQL Server source connector (`tiberius`, TDS) | a TLS backend (rustls, bundled) |
| `oracle` | real Oracle source connector (`oracle`/ODPI-C) | Oracle Instant Client at runtime |
| `mongodb` | real MongoDB source connector (`mongodb` async driver) | none (bundled bson + TLS) |
| `kafka-lag` | precise CDC offset-lag (embedded Kafka client) in the reconciler | `librdkafka` via `cmake` |

Build the gateway with, e.g., `cargo build -p atlas-gateway --features atlas-databridge/sqlserver,atlas-databridge/oracle,atlas-databridge/mongodb,atlas-databridge/kafka-lag`.

## Testing
- **Fake pipeline (no infra)**: `cargo test --workspace` covers the unit tests plus the gateway
  integration suites — `tests/databridge_pipeline.rs` (Postgres state machine + cutover guard) and
  `tests/databridge_engines.rs` (all six engines discover→validate + correct edge routing:
  cnpg/percona/psmdb).
- **Real source connectors (containers)**: each engine has an env-gated `#[tokio::test]` that runs only
  when its `DATABRIDGE_TEST_*` var is set (skips otherwise), mirroring `discovers_real_postgres`:

  | Engine | Var (value: `host,port,database,user,password`) | Feature |
  |---|---|---|
  | Postgres | `DATABRIDGE_TEST_PG` (libpq conn string) | *(default)* |
  | MySQL | `DATABRIDGE_TEST_MYSQL` | *(default)* |
  | MariaDB | `DATABRIDGE_TEST_MARIADB` | *(default)* |
  | MongoDB | `DATABRIDGE_TEST_MONGO` | `mongodb` |
  | SQL Server | `DATABRIDGE_TEST_MSSQL` | `sqlserver` |

  **`scripts/test-connectors.sh`** automates this: it spins each engine as an ephemeral podman/docker
  container, seeds a `customers`/`orders` schema, exports the var, runs the gated test, and tears the
  container down (`scripts/test-connectors.sh [pg mysql mariadb mongo mssql]`). Oracle is compile-only
  (needs the OCI client). **`scripts/test-all.sh`** runs the whole gate (clippy + workspace tests +
  feature compiles + the container connector tests).

## Data model
`migration_sources`, `migration_plans`, `edge_db_clusters`, `cdc_streams`, `validation_runs`,
`cutovers` (SQLite; migration `0011_databridge.sql`). Plan state machine:
`draft → discovered → assessed → provisioning → provisioned → full_loading → loaded →
cdc_streaming → validating → validated → cutover_pending → cutover_in_progress → cutover_complete
→ completed | rolled_back | failed`.

## Live verification (real infra)
**Verified end-to-end on two independent live Rook Ceph clusters** (`175.110.122.71`, `80.79.5.173`).
A full real migration was driven through the deployed gateway (k3s + Rook Ceph), Postgres →
Ceph-backed edge Postgres:

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

**CDC (Strimzi/Debezium) — VERIFIED end-to-end live.** On the Rook cluster: Strimzi + a KRaft Kafka +
a custom Connect image (Debezium PostgreSQL + Aiven JDBC sink, `deploy/databridge/connect/Dockerfile`,
set via `ATLAS_DATABRIDGE_CONNECT_IMAGE`). The gateway's `cdc/start` created the KafkaConnect + Debezium
source + JDBC-sink connectors; a row inserted into the source Postgres **replicated to the Ceph-backed
edge Postgres** (source → Debezium → Kafka → JDBC sink → edge), and continuous inserts converged.

Getting there corrected **ten real issues** in the blind-built CDC path (all now in the code / deploy),
confirmed by re-running the whole flow on the **second** cluster: `cdc/start` there produced both
connectors RUNNING and replicated a row to the edge with **zero manual patching** — the committed code
+ `deploy/databridge/` bundle stand up working CDC on their own.
1. Strimzi serves the kinds at `kafka.strimzi.io/v1` (not `v1beta2`);
2. bootstrap Service is `<kafka>-kafka-bootstrap:9092`;
3. `KafkaConnect` needs top-level `groupId` + `*StorageTopic` (not under `config`);
4. Connect image must match the operator's Kafka major (4.x base) + Debezium 3.x;
5. single-broker lab needs `{config,offset,status}.storage.replication.factor: 1`;
6. `KafkaConnect` needs `strimzi.io/use-connector-resources: "true"` or connectors are ignored;
7. the Connect ServiceAccount needs RBAC to `get` Secrets (`deploy/databridge/connect-rbac.yaml`);
8. the JDBC sink needs the Debezium `ExtractNewRecordState` unwrap SMT + a `RegexRouter` (topic→table)
   + `pk.mode=record_key`/`pk.fields`;
9. Debezium must use `decimal.handling.mode=double` (Postgres unscaled `numeric` → a STRUCT the sink
   can't bind) + `snapshot.mode=never` (the full-load already seeded the edge);
10. the JDBC sink needs `consumer.override.metadata.max.age.ms` shortened (default 5 min) so it discovers
    a table's Debezium topic (created on the first change) promptly, without a restart.

Real CDC is **health-tracked** by the reconciler: a streaming stream backed by a real Debezium
`KafkaConnector` follows the connector's state (RUNNING → live, else → `error`); fake
streams keep the synthesized lag drain. The sink `pk.fields` is configurable via
`ATLAS_DATABRIDGE_SINK_PK_FIELDS` (default `id`). With the **`kafka-lag`** feature, a healthy real
stream also reports its **precise offset-lag** (∑ per-partition `high_watermark − committed_offset` of
the sink consumer group `connect-<sink-connector>`) via an embedded Kafka client; without it, a healthy
stream reports caught-up (0).

## Status
- **Done + verified end-to-end on two live Rook Ceph clusters** (and CI-locked by
  `tests/databridge_pipeline.rs` for the fake path): the entire pipeline — discover (real
  `tokio-postgres` connector) → assess → provision (CNPG/Percona CR on Ceph, reconciler-advanced) →
  full-load (`pg_dump|psql` / `mysqldump|mysql` batch Jobs, reconciler-watched) → **CDC** (Strimzi
  `KafkaConnect` + Debezium source + JDBC-sink connectors, live source→edge replication) → validate
  (row-count-compare Job) → cutover (guarded) → rollback.
- Real CDC is **health-tracked** by the reconciler; the JDBC sink `pk.fields` is configurable.
- **Engine coverage**: all six source engines (Postgres, MySQL, MariaDB, Oracle, SQL Server, MongoDB)
  run the full pipeline; the fake path for every engine is exercised end-to-end (discover→…→cutover).
  Real Postgres/MySQL/MariaDB connectors are default; real SQL Server (`tiberius`), Oracle (`oracle`/OCI)
  and MongoDB (`mongodb` driver) are behind cargo features; precise CDC lag is behind `kafka-lag`.
  Oracle/SQL Server are heterogeneous (→ Postgres edge, seeded by the Debezium initial snapshot);
  MongoDB is a homogeneous document migration (→ Percona Server for MongoDB, Mongo Kafka sink).
- **Day-2 self-heal**: a stalled/errored CDC stream is re-established by `POST .../cdc/restart` (bumps
  the stream's `restart_count` and re-applies the connector CRs); the reconciler also **auto-restarts**
  an unhealthy stream up to 3 times before giving up (→ `error`, which the monitor's CDC-error rule then
  alerts on).
- **MySQL / MariaDB / MongoDB / SQL Server real-connector discovery — verified live** (2026-07): real
  connector discovery run against a real MySQL 8.4, MariaDB 11.x, MongoDB 7.0 (single-member replica set),
  and SQL Server 2022 (CDC-enabled) on the deployed k3s gateway — engine/version, `cdc_capable` (binlog
  ROW for MySQL/MariaDB; replica-set change streams for Mongo; `sys.databases.is_cdc_enabled` for SQL
  Server), real database names, and real per-table/collection schema+name with PK / document counts all
  returned correctly. The MySQL/MariaDB run surfaced and fixed a real bug: sqlx-mysql won't decode
  `information_schema` string columns (binary-ish collation) as `String`, so the names came back empty
  until the queries were changed to `CAST(... AS CHAR)` (the fake tests couldn't catch it — canned data).
- **Oracle real-connector discovery — verified live** (2026-07): the gateway image now bundles the OCI
  Instant Client (Basic Lite) and builds the `oracle` feature; discovery against a real Oracle 23ai/26ai
  Free (`gvenzl/oracle-free`) returned engine/version, supplemental-log-min `cdc_capable`, real user
  schemas, and the real user tables with PKs. **All six source engines are now discovery-verified live.**
- **System-schema filtering — fixed (surfaced live)**: real Oracle/SQL Server discovery leaked internal
  objects (Oracle 23ai/26ai `VECSYS`/`DBSFWUSER`/`BAASSYS`/…; SQL Server `cdc.*` + `dbo.systranschemas`)
  as migratable tables. Fixed to use each engine's own metadata flag — Oracle `all_users.oracle_maintained
  = 'N'` and SQL Server `sys.tables.is_ms_shipped = 0` (+ excluding the `cdc`/`sys` schemas) — instead of
  a fragile hardcoded denylist.
- **MySQL full-load + validate — verified live** (2026-07): drove the real gateway pipeline
  register → discover → assess → provision → full-load → validate against a lightweight edge MySQL
  (standing in for the Percona edge: the gateway created the real `PerconaXtraDBCluster` CR, and the
  reconciler advanced the edge to `ready` off the CR's status). The **full-load ran the real
  `mysqldump→mysql` batch Job and physically copied the data** (3 customers + 2 orders) source→edge, and
  the **validate row-count-compare Job passed** (plan → `validated`). Streaming CDC + cutover were not
  run — they need Kafka/Debezium, which is not installed on the shared lab.
- **Follow-ups (verify on live infra)**: **streaming CDC + cutover** for the non-Postgres engines, and
  full-load for MariaDB/MongoDB/SQL Server/Oracle (MySQL is the one verified through full-load+validate;
  Postgres through CDC) — the Kafka/Debezium/edge-operator stack is not installed on the shared k3s lab,
  so these remain wired + unit-tested but not yet live-verified. cross-engine per-table row-count
  validation for heterogeneous plans is advisory (parity confirmed by snapshot/stream convergence
  rather than a source-vs-edge count Job). Real MongoDB CDC needs a Connect image bundling the Debezium
  MongoDB connector + the MongoDB Kafka sink.

### Full-load secret assumptions
The real full-load Job runs in `zyvor-databridge`, so the **source Secret must exist in that
namespace** with `username`/`password` keys. The edge Secret is operator-generated: CloudNativePG's
`<cluster>-app` provides a ready-to-use `uri`; Percona's `<cluster>-secrets` provides `root`.
