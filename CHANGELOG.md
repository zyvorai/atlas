<!-- Copyright (c) 2026 ZyvorAI Labs Private Limited. -->
<!-- SPDX-License-Identifier: AGPL-3.0-only OR LicenseRef-Atlas-Commercial -->
# Changelog

All notable changes to Atlas will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/). Versions
before `0.2.0` were not tracked here — see `git log` for that history.

## [Unreleased]

### Changed — Dual license (AGPL-3.0 + ACL); remove trial JWT gate

- Open-source under [AGPL-3.0](LICENSE); commercial track via
  [Atlas Commercial License (ACL)](COMMERCIAL_LICENSE.md). See [docs/LICENSING.md](docs/LICENSING.md).
- Removed Ed25519 JWT trial stack: `atlas-license`, `atlas-license-tool`, gateway
  `license_middleware` / `GET /license/status`, `LicenseBanner`, deploy Secret wiring, and
  `ATLAS_LICENSE_ENFORCE`. AGPL self-host is ungated (same model as Aurora).
- Per-file `SPDX-License-Identifier: AGPL-3.0-only OR LicenseRef-Atlas-Commercial` headers;
  [`LICENSES/`](LICENSES/), [`NOTICE`](NOTICE), [`CLA.md`](CLA.md), [`DCO.md`](DCO.md);
  `make headers` / CI license-header lint.

### Added — Relay product ownership

- Document `relay` as an Atlas product consumer in `docs/PRODUCTS.md` (`owner.product=relay`,
  `resource_type=database`, `role=data_disk`). Relay keeps its Postgres ledger; Atlas provisions
  the data PVC and optional RGW backups. Implementation lives in the Relay repo
  (`docs/ATLAS_STORAGE.md`, `scripts/atlas-provision-relay-storage.sh`).

## [0.2.0] — 2026-08-25

### Added — Trial/licensing (Ed25519-signed JWT)

- Same design as sibling Zyvor products Veyron (`trial.rs`) and Aurora
  (`gtm_api.middleware.license`): a signed token carries who it was issued to and when it
  expires, verified against an embedded public key — no server-side clock, so deleting local
  state can't extend a trial. New `atlas-license` crate (verify/status, product tag
  `atlas-trial`) and `atlas-license-tool` (sales-only `keygen`/`issue` CLI).
- Gateway wiring: `license_middleware` gates the bearer-protected API with `402` once a trial
  expires; `GET /license/status` stays reachable alongside `/auth/login` and `/auth/oidc/*`
  so an expired install can still show *why*. `LicenseBanner.tsx` in the console UI.
  `Config::license_enforce` defaults to enforced (matching Aurora); local dev
  (`make run`/`make run-databridge`) and both `deploy/k8s/atlas-gateway*.yaml` manifests
  explicitly opt out until a real customer token exists. See `docs/LICENSING.md`.
- `Config::validate_for_start()` now warns (non-fatally) at boot if enforcement is on with no
  locatable token, instead of silently 402ing every protected route with no signal.

### Fixed — Cross-tenant write vulnerability across volumes, buckets, and direct RBD

A real, exploitable pre-existing gap: the tenant-scoping refactor (`require_tenant`/
`tenant_scope` in `auth.rs`) was applied to read handlers but never propagated to write
handlers. A tenant-scoped operator could create a volume or RBD image attributed to (and
quota-charged against) a *different* tenant, or expand/snapshot/clone/restore/resize/migrate/
flatten any tenant's volume or RBD image by id, or delete/prune/download any tenant's bucket,
backup, or bucket object — including minting a presigned S3 download URL for another tenant's
backup data. Fixed at every affected call site in `volumes.rs`, `object_store.rs`, and
`rbd.rs`; centralized in `object_store.rs`'s shared `bucket_s3_target()` helper so every object
operation is covered by one check. Admin-only operations are unchanged (admin is deliberately
global everywhere else in this codebase). New regression test
`operator_cannot_write_across_tenants` in `tenant_isolation.rs`.

### Added — UI lint, component tests, and CI gates

- `eslint.config.js` (flat config) for the gateway console — previously had none. CI now runs
  `npm run lint` + `npm run test` + `npm audit --audit-level=high` (previously only
  `npm run build`); the audit gate found and fixed 2 pre-existing high-severity advisories
  (nanoid, react-router) via a non-breaking patch bump.
- First component tests (`@testing-library/react` + jsdom) — `LicenseBanner.test.tsx`, guarding
  the licensed/expired/days-remaining states after that component shipped with zero tests and
  one real bug (see below) caught only by manual live verification.

### Fixed — License status couldn't distinguish "expired" from "never had a token"

Caught during live verification of the trial/licensing feature above, before it shipped:
`GET /license/status` reported an expired-but-authentically-signed token identically to no
token being present at all (`trial_expired: false` either way), because `jsonwebtoken`'s
built-in `validate_exp` rejected the token before its claims could be inspected. Since
`LicenseBanner.tsx`'s render logic branches on `trial_expired`, this would have silently kept
the "your trial has ended" message from ever appearing. Fixed by deferring expiry checking to
`atlas-license::status()` (which has the real `exp` claim to work from) instead of
`jsonwebtoken`'s decode-time rejection; regression-tested on both the Rust and TypeScript sides.

### Fixed — Real MySQL DataBridge CDC, unrunnable end-to-end before this pass

Live-verified `discover → full-load → cdc/start → validate` against a real Percona edge and a
real Kafka/Strimzi/Debezium stack for the first time; the code path had never been run against
live infra. Found and fixed 8 real bugs: `mysql_operator.rs`'s PXC CR builder never set
`allowUnsafeConfigurations` (blocks a single-node edge from ever reporting `ready`); `loader.rs`'s
`mysqldump` pipeline was missing `CREATE DATABASE` (target db doesn't exist yet), `--no-tablespaces`
(source creds lack `PROCESS`), `--skip-add-locks` (PXC's `pxc_strict_mode` rejects `LOCK TABLES`),
and `--set-gtid-purged=OFF` (conflicts with the edge's own GTID set); `streaming.rs`'s
`connect_spec()` used Strimzi's ~90s-to-kill default probe, too tight for a Debezium+JDBC Connect
image to boot; `streaming.rs`'s Debezium source config was missing `time.precision.mode: connect`;
`deploy/databridge/connect/Dockerfile` pinned Debezium 3.0.8.Final against a Kafka 4.3.0 base
image whose client library removed a method 3.0.8's schema-history recovery depends on
(`NoSuchMethodError`) — bumped to 3.3.0.Final. See `docs/DATABRIDGE.md` for two further
non-code operational findings surfaced getting a row to actually land on the edge: a failed
sink message permanently poisons its Kafka consumer offset (a task restart alone never skips
past it), and Debezium unconditionally encodes MySQL `TIMESTAMP` columns (not `DATETIME`) as
ISO-8601 strings the JDBC sink can't bind — a genuine open gap for any real schema using
`TIMESTAMP`, not yet fixed.
