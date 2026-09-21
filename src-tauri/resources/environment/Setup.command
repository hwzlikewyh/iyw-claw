#!/bin/sh
set -eu
directory=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
helper="$directory/../../MacOS/iyw-environment"
if [ ! -x "$helper" ]; then
  printf '%s\n' 'The environment installer is missing. Reinstall the application.' >&2
  exit 1
fi
version=$("$helper" --application-version)
if "$helper" repair --app-version "$version"; then
  printf '%s\n' 'Environment installation completed.'
else
  code=$?
  printf '%s\n' 'Environment installation failed. See the component error above.' >&2
  if [ -t 0 ]; then printf '%s' 'Press Enter to close: '; read -r reply; fi
  exit "$code"
fi
