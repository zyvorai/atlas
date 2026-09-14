#!/usr/bin/env bash
# Copyright (c) 2026 ZyvorAI Labs Private Limited.
# SPDX-License-Identifier: AGPL-3.0-only OR LicenseRef-Atlas-Commercial
# Stand up a throwaway Postgres in the lab k3s cluster to verify atlas-inventory's Phase-1 HA
# scaffolding (connect_postgres/migrate_postgres, docs/HA.md) against real infrastructure — not
# just a compile-check behind the `postgres` cargo feature. See deployment.yaml.
#
# Usage (run ON the lab host, or with kubectl already pointed at it):
#   ./up.sh
#
# What you get:
#   - Namespace zyvor-system (created if missing)
#   - Secret postgres-lab-auth with a freshly generated password (never printed, never committed)
#   - PVC + Deployment + Service postgres-lab on NodePort 30432
#
# This is a lab-only smoke test target: single replica, ephemeral local-path storage, generated
# credential. It exists to prove connect_postgres()/migrate_postgres() work against a real Postgres,
# not to model a production HA topology (see docs/HA.md for what a real port would still need).
set -euo pipefail
cd "$(dirname "$0")"
NS=zyvor-system

log() { printf '\033[1;36m==> %s\033[0m\n' "$*"; }

log "namespace"
kubectl create namespace "$NS" --dry-run=client -o yaml | kubectl apply -f -

log "auth secret (generated, idempotent — leaves an existing secret alone)"
if kubectl -n "$NS" get secret postgres-lab-auth >/dev/null 2>&1; then
  echo "postgres-lab-auth already exists, leaving it as-is"
else
  PASS="$(openssl rand -base64 24 | tr -d '\n=/+' | head -c 24)"
  kubectl -n "$NS" create secret generic postgres-lab-auth \
    --from-literal=password="$PASS" \
    --from-literal=database-url="postgres://atlas:${PASS}@postgres-lab.${NS}.svc:5432/atlas"
  unset PASS
fi

log "PVC + Deployment + Service"
kubectl apply -f deployment.yaml
kubectl -n "$NS" rollout status deploy/postgres-lab --timeout=180s

NODE_IP="$(kubectl get nodes -o jsonpath='{.items[0].status.addresses[?(@.type=="InternalIP")].address}')"
cat <<EOF

postgres-lab is up:
  External:    postgres://atlas:<password>@${NODE_IP}:30432/atlas
  In-cluster:  postgres://atlas:<password>@postgres-lab.${NS}.svc:5432/atlas

Password (and a ready-to-use DATABASE_URL) are in the postgres-lab-auth Secret:
  kubectl -n ${NS} get secret postgres-lab-auth -o jsonpath='{.data.database-url}' | base64 -d

Verify connect_postgres()/migrate_postgres() against it:
  DATABASE_URL="\$(kubectl -n ${NS} get secret postgres-lab-auth -o jsonpath='{.data.database-url}' | base64 -d | sed 's#postgres-lab.${NS}.svc:5432#${NODE_IP}:30432#')"
  cargo test -p atlas-inventory --features postgres --test postgres_live -- --ignored --nocapture

Tear down when done (this is a throwaway lab target, not a standing service):
  kubectl -n ${NS} delete deploy/postgres-lab svc/postgres-lab pvc/postgres-lab-data secret/postgres-lab-auth
EOF
