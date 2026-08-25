# Licensing

Atlas evaluation uses a **signed trial token** (Ed25519 / EdDSA JWT) — the same design as
Veyron and Aurora across the Zyvor product family. There is **no** server-side clock: expiry
lives inside the token. Deleting local state cannot extend a trial.

The gateway embeds only the **public** key (`atlas_license::LICENSE_PUBLIC_KEY_B64`). Zyvor
sales holds the private key and issues `trial.token` files via `atlas-license-tool`.

## How the trial works

- Deployments run with a `trial.token` (or `ATLAS_TRIAL_TOKEN` / `ATLAS_LICENSE_KEY` env value)
  set by whoever deployed them, typically 30 days.
- While the token is valid, every feature works — nothing is crippled.
- After `exp`, protected REST routes return **HTTP 402** until a renewed signed token is
  installed. `/health`, `/livez`, `/readyz`, `/version`, `GET /license/status`,
  `POST /auth/login`, and `/auth/oidc/*` stay reachable so an expired install can still show the
  operator why everything else is gated, and so a fresh token can be applied without redeploying
  code.

**Contact:** [sales@zyvor.dev](mailto:sales@zyvor.dev)

## Apply a token

| Path | How |
|------|-----|
| Env | `ATLAS_TRIAL_TOKEN=<jwt>` (or `ATLAS_LICENSE_KEY=<jwt>`) then restart |
| File path | `ATLAS_TRIAL_TOKEN_FILE=/path/to/trial.token` |
| Default file | `trial.token` next to the gateway's working directory |
| Kubernetes | `secretKeyRef` into `ATLAS_TRIAL_TOKEN` — see `deploy/k8s/atlas-gateway.yaml`'s
`atlas-license` Secret block (same pattern already used for `ATLAS_OIDC_CLIENT_SECRET`) |

Status (always reachable, expired or not):

```bash
curl -s http://localhost:5110/api/atlas/v1/license/status
```

## Local development

```bash
ATLAS_LICENSE_ENFORCE=false   # skip the middleware entirely (make run / make run-databridge already set this)
```

`make run`/`make run-databridge` already export `ATLAS_LICENSE_ENFORCE=false` so local `no
Ceph, no cluster needed` dev keeps working with zero setup. The code-level default when the env
var is unset is `true` (enforced) — the same default Aurora ships.

## Sales: minting tokens (private repo / sales tooling only)

Do **not** ship `crates/atlas-license-tool` or `secrets/` in customer packages — both are
already gitignored (`.gitignore`: `/secrets/`, `*.pkcs8`, `trial.token`).

```bash
cargo run -p atlas-license-tool -- keygen                                  # once; paste public key into crates/atlas-license/src/lib.rs
cargo run -p atlas-license-tool -- issue --who "Acme Corp" --days 30 -o trial.token
```

Product claim: `atlas-trial` (tokens issued for Veyron/Aurora/Ragnarok will not unlock Atlas,
and vice versa — every product embeds its own public key and rejects any other product's
`product` claim).

## Rotating the signing key

Regenerate with `cargo run -p atlas-license-tool -- keygen`, replace
`LICENSE_PUBLIC_KEY_B64` in `crates/atlas-license/src/lib.rs`, and reissue any trials/licenses
still active — rotation invalidates every previously issued token immediately on the next
gateway restart with the new binary.
