#!/usr/bin/env bash

set -euo pipefail

required=(
  TAURI_SIGNING_PRIVATE_KEY
  TAURI_SIGNING_PRIVATE_KEY_PASSWORD
  APPLE_CERTIFICATE
  APPLE_CERTIFICATE_PASSWORD
  KEYCHAIN_PASSWORD
  APPLE_ID
  APPLE_PASSWORD
  APPLE_TEAM_ID
)
missing=()

for name in "${required[@]}"; do
  if [[ -z "${!name:-}" ]]; then
    missing+=("$name")
  fi
done

if [[ "${#missing[@]}" -ne 0 ]]; then
  printf '::error::Missing required release secret(s): %s\n' "${missing[*]}"
  exit 1
fi
