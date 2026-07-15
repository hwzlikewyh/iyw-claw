#!/usr/bin/env bash

set -euo pipefail

keychain_path="$RUNNER_TEMP/iyw-claw-signing.keychain-db"
certificate_path="$RUNNER_TEMP/developer-id.p12"
search_list_path="$RUNNER_TEMP/iyw-claw-keychains.txt"
original_default=$(
  security default-keychain -d user \
    | sed -e 's/^[[:space:]]*//' -e 's/^"//' -e 's/"$//'
)
security list-keychains -d user \
  | sed -e 's/^[[:space:]]*//' -e 's/^"//' -e 's/"$//' \
  > "$search_list_path"

cat >> "$GITHUB_ENV" <<EOF
APPLE_KEYCHAIN_PATH=$keychain_path
APPLE_CERTIFICATE_PATH=$certificate_path
APPLE_KEYCHAIN_SEARCH_LIST_PATH=$search_list_path
APPLE_ORIGINAL_DEFAULT_KEYCHAIN=$original_default
EOF

echo "$APPLE_CERTIFICATE" | base64 --decode > "$certificate_path"
security create-keychain -p "$KEYCHAIN_PASSWORD" "$keychain_path"
existing_keychains=()
while IFS= read -r existing_keychain; do
  if [[ -n "$existing_keychain" ]]; then
    existing_keychains+=("$existing_keychain")
  fi
done < "$search_list_path"
if [[ "${#existing_keychains[@]}" -eq 0 ]]; then
  security list-keychains -d user -s "$keychain_path"
else
  security list-keychains -d user -s "$keychain_path" "${existing_keychains[@]}"
fi
security default-keychain -s "$keychain_path"
security unlock-keychain -p "$KEYCHAIN_PASSWORD" "$keychain_path"
security set-keychain-settings -t 21600 -u "$keychain_path"
security import "$certificate_path" \
  -k "$keychain_path" \
  -P "$APPLE_CERTIFICATE_PASSWORD" \
  -T /usr/bin/codesign \
  -T /usr/bin/productbuild
security set-key-partition-list \
  -S apple-tool:,apple:,codesign: \
  -s \
  -k "$KEYCHAIN_PASSWORD" \
  "$keychain_path"
