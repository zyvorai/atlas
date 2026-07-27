#!/usr/bin/env bash
# Copyright (c) 2026 ZyvorAI Labs Private Limited. All rights reserved.
# Auth: 401 without token; mint + revoke.
set -euo pipefail
# shellcheck source=lib.sh
source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/lib.sh"

section "02-auth"

_saved="${ATLAS_TOKEN:-}"
ATLAS_TOKEN=""
assert_code 401 GET /pools || true
ATLAS_TOKEN="$_saved"

assert_ok POST /auth/tokens \
  '{"subject":"live-revoke-me","role":"viewer","ttl_secs":600}' || true
JTI="$(json_field jti)"
if [[ -n "$JTI" ]]; then
  cleanup_register "token:${JTI}"
  assert_ok POST "/auth/tokens/${JTI}/revoke" '{}' || true
fi
assert_ok GET /auth/tokens/revoked || true
