#!/bin/sh
set -eu
directory=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
helper="$directory/../../../bin/iyw-environment"
if [ ! -x "$helper" ] && [ -n "${APPDIR:-}" ]; then
  helper="$APPDIR/usr/bin/iyw-environment"
fi
if [ ! -x "$helper" ]; then
  printf '%s\n' 'The environment installer is missing. Reinstall the application.' >&2
  exit 1
fi
version=$("$helper" --application-version)
exec "$helper" repair --app-version "$version"
