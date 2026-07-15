#!/usr/bin/env bash

set -euo pipefail

required=(
  TAURI_SIGNING_PRIVATE_KEY
  TAURI_SIGNING_PRIVATE_KEY_PASSWORD
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
