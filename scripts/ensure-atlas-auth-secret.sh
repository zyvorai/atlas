#!/usr/bin/env bash
# Copyright (c) 2026 ZyvorAI Labs Private Limited. All rights reserved.
#
# Ensure Secret `atlas-gateway-auth` exists in NAMESPACE with a strong jwt-secret and an optional
# bootstrap-admin-token. Idempotent: never rotates an existing secret.
#
# Usage (on a host with kubectl):
#   NAMESPACE=zyvor-system bash scripts/ensure-atlas-auth-secret.sh
#   NAMESPACE=rook-ceph    bash scripts/ensure-atlas-auth-secret.sh
#
# Prints the bootstrap token to stdout once when the Secret is created (so the operator can mint
# lasting JWTs). Re-runs that find an existing Secret print nothing sensitive.
set -euo pipefail

NS="${NAMESPACE:?set NAMESPACE (e.g. zyvor-system or rook-ceph)}"
NAME="${SECRET_NAME:-atlas-gateway-auth}"
KUBECTL="${KUBECTL:-kubectl}"

if $KUBECTL -n "$NS" get secret "$NAME" >/dev/null 2>&1; then
  echo "secret/${NAME} already exists in ${NS} — left alone" >&2
  exit 0
fi

JWT="$(openssl rand -base64 48 | tr -d '\n')"
BOOT="$(openssl rand -base64 32 | tr -d '\n=/+' | head -c 40)"
$KUBECTL -n "$NS" create secret generic "$NAME" \
  --from-literal=jwt-secret="$JWT" \
  --from-literal=bootstrap-admin-token="$BOOT" >/dev/null

cat >&2 <<EOF
created secret/${NAME} in ${NS}
  jwt-secret:               (stored in Secret; not printed)
  bootstrap-admin-token:    ${BOOT}

Use once to mint lasting JWTs, then remove the bootstrap key:
  curl -sS -H "Authorization: Bearer ${BOOT}" \\
    -H 'Content-Type: application/json' \\
    -d '{"subject":"ops","role":"admin","ttl_secs":86400}' \\
    http://<node>:<port>/api/atlas/v1/auth/tokens

  $KUBECTL -n ${NS} patch secret ${NAME} --type=json \\
    -p='[{"op":"remove","path":"/data/bootstrap-admin-token"}]'
  $KUBECTL -n ${NS} rollout restart deploy/<atlas-gateway>
EOF

# Machine-readable line for deploy scripts that want to verify with the token.
echo "BOOTSTRAP_ADMIN_TOKEN=${BOOT}"
