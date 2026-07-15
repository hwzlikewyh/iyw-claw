#!/usr/bin/env bash

set -euo pipefail

cert_info=$(
  security find-identity -v -p codesigning "$APPLE_KEYCHAIN_PATH" \
    | grep "Developer ID Application" \
    | head -n 1 \
    || true
)
if [[ -z "$cert_info" ]]; then
  echo "::error::No Developer ID Application signing identity found."
  security find-identity -v -p codesigning "$APPLE_KEYCHAIN_PATH" || true
  exit 1
fi

cert_id=$(echo "$cert_info" | awk -F'"' '{print $2}')
echo "APPLE_SIGNING_IDENTITY=$cert_id" >> "$GITHUB_ENV"
echo "Using Apple signing identity: $cert_id"
