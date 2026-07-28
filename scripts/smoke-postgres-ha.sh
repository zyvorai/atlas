#!/usr/bin/env bash
# Copyright (c) 2026 ZyvorAI Labs Private Limited. All rights reserved.
# Phase-1 HA smoke: compile atlas-inventory with `postgres`, optionally migrate a live DB.
#
# Usage:
#   ./scripts/smoke-postgres-ha.sh              # cargo check + unit/integration (no live DB)
#   ./scripts/smoke-postgres-ha.sh --migrate    # also run ignored migrate test (needs Postgres)
#
# Lab Postgres (optional — not required for --check-only path):
#   docker compose -f deploy/postgres/docker-compose.yml up -d
#   export DATABASE_URL=postgres://atlas:atlas@127.0.0.1:5432/atlas
#
# If Docker is unavailable, skip --migrate; cargo check still validates the feature builds.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

RUN_MIGRATE=0
for arg in "$@"; do
  case "$arg" in
    --migrate) RUN_MIGRATE=1 ;;
    -h|--help)
      sed -n '2,16p' "$0"
      exit 0
      ;;
  esac
done

echo "==> cargo check -p atlas-inventory --features postgres"
cargo check -p atlas-inventory --features postgres

echo "==> cargo test -p atlas-inventory --features postgres (non-ignored)"
cargo test -p atlas-inventory --features postgres --test postgres_connect

if [[ "$RUN_MIGRATE" -eq 1 ]]; then
  URL="${DATABASE_URL:-${ATLAS_DATABASE_URL:-}}"
  if [[ -z "$URL" ]]; then
    echo "ERROR: --migrate needs DATABASE_URL or ATLAS_DATABASE_URL (postgres://...)" >&2
    echo "Start lab DB with: docker compose -f deploy/postgres/docker-compose.yml up -d" >&2
    exit 1
  fi
  echo "==> migrate against $URL"
  DATABASE_URL="$URL" cargo test -p atlas-inventory --features postgres \
    --test postgres_connect connect_and_migrate_postgres -- --ignored --nocapture
else
  echo "==> skip live migrate (pass --migrate + DATABASE_URL when Docker/Postgres is up)"
fi

echo "OK: Phase-1 postgres feature smoke passed"
