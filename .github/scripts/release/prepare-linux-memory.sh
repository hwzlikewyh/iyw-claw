#!/usr/bin/env bash
set -euo pipefail

if [[ "${RUNNER_ENVIRONMENT:-}" != "github-hosted" || "$OSTYPE" != linux* ]]; then
  echo "[build-memory] keeping the existing runner memory configuration"
  exit 0
fi

readonly GIB_BYTES=$((1024 * 1024 * 1024))
readonly DESIRED_SWAP_GIB=24
readonly DISK_RESERVE_GIB=16
readonly MIN_ADDITIONAL_SWAP_GIB=8
readonly swap_file="${RUNNER_TEMP:?}/iyw-desktop-build.swap"
existing_swap_kib=$(awk '/^SwapTotal:/ { print $2 }' /proc/meminfo)
existing_swap_gib=$((existing_swap_kib * 1024 / GIB_BYTES))
if (( existing_swap_gib >= DESIRED_SWAP_GIB )); then
  echo "[build-memory] existing swap meets the ${DESIRED_SWAP_GIB} GiB target"
  exit 0
fi

available_bytes=$(df -B1 --output=avail "$RUNNER_TEMP" | awk 'NR == 2 { print $1 }')
available_gib=$((available_bytes / GIB_BYTES))
additional_gib=$((DESIRED_SWAP_GIB - existing_swap_gib))
disk_budget_gib=$((available_gib - DISK_RESERVE_GIB))
if (( additional_gib > disk_budget_gib )); then
  additional_gib=$disk_budget_gib
fi
if (( additional_gib < MIN_ADDITIONAL_SWAP_GIB )); then
  echo "::error::Not enough disk for build swap: available=${available_gib} GiB; reserved=${DISK_RESERVE_GIB} GiB"
  exit 1
fi
if [[ -e "$swap_file" ]]; then
  echo "::error::Refusing to replace an existing build swap file"
  exit 1
fi

echo "[build-memory] existing_swap=${existing_swap_gib} GiB; additional_swap=${additional_gib} GiB; reserved_disk=${DISK_RESERVE_GIB} GiB"
sudo fallocate -l "$((additional_gib * GIB_BYTES))" "$swap_file"
sudo chmod 600 "$swap_file"
sudo mkswap "$swap_file"
sudo swapon "$swap_file"
free -h
df -h "$RUNNER_TEMP"
