# Changelog

All notable changes to Atlas will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/). Versions
before `0.2.0` were not tracked here — see `git log` for that history.

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
