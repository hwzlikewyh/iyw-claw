#!/usr/bin/env bash

if [[ -n "${APPLE_ORIGINAL_DEFAULT_KEYCHAIN:-}" ]]; then
  security default-keychain -s "$APPLE_ORIGINAL_DEFAULT_KEYCHAIN" || true
fi

if [[ -f "${APPLE_KEYCHAIN_SEARCH_LIST_PATH:-}" ]]; then
  restored=()
  while IFS= read -r keychain; do
    if [[ -n "$keychain" ]]; then
      restored+=("$keychain")
    fi
  done < "$APPLE_KEYCHAIN_SEARCH_LIST_PATH"
  if [[ "${#restored[@]}" -ne 0 ]]; then
    security list-keychains -d user -s "${restored[@]}" || true
  fi
fi

if [[ -n "${APPLE_KEYCHAIN_PATH:-}" ]]; then
  security delete-keychain "$APPLE_KEYCHAIN_PATH" || true
fi

for path in \
  "${APPLE_CERTIFICATE_PATH:-}" \
  "${APPLE_KEYCHAIN_SEARCH_LIST_PATH:-}"; do
  if [[ -n "$path" ]]; then
    rm -f "$path"
  fi
done
