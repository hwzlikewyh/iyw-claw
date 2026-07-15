#!/usr/bin/env bash

set -euo pipefail

if [[ ! "$RELEASE_TAG" =~ ^v(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)(-[0-9A-Za-z][0-9A-Za-z.-]*)?$ ]]; then
  echo "::error::Release tag '$RELEASE_TAG' is not a supported semantic version."
  exit 1
fi

if ! git show-ref --verify --quiet "refs/tags/$RELEASE_TAG"; then
  echo "::error::Release tag '$RELEASE_TAG' does not exist."
  exit 1
fi

git fetch origin "$DEFAULT_BRANCH"
tag_commit=$(git rev-list -n 1 "$RELEASE_TAG")
if ! git merge-base --is-ancestor "$tag_commit" "origin/$DEFAULT_BRANCH"; then
  echo "::error::Tag $RELEASE_TAG is not based on $DEFAULT_BRANCH"
  exit 1
fi

tag_version="${RELEASE_TAG#v}"
package_version=$(node -p "require('./package.json').version")
tauri_version=$(node -p "require('./src-tauri/tauri.conf.json').version")
cargo_version=$(
  cargo metadata \
    --manifest-path src-tauri/Cargo.toml \
    --no-deps \
    --format-version 1 \
    | node -e '
        const fs = require("node:fs");
        const metadata = JSON.parse(fs.readFileSync(0, "utf8"));
        const app = metadata.packages.find((item) => item.name === "iyw-claw");
        if (!app) process.exit(1);
        process.stdout.write(app.version);
      '
)

for entry in \
  "package.json:$package_version" \
  "src-tauri/tauri.conf.json:$tauri_version" \
  "src-tauri/Cargo.toml:$cargo_version"; do
  file="${entry%%:*}"
  version="${entry#*:}"
  if [[ "$version" != "$tag_version" ]]; then
    echo "::error::$file version $version does not match tag $RELEASE_TAG"
    exit 1
  fi
done

prerelease=false
if [[ "$RELEASE_TAG" == *-* ]]; then
  prerelease=true
fi

echo "release_tag=$RELEASE_TAG" >> "$GITHUB_OUTPUT"
echo "prerelease=$prerelease" >> "$GITHUB_OUTPUT"
echo "Tag $RELEASE_TAG is valid and based on $DEFAULT_BRANCH"
