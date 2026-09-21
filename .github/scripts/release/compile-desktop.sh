#!/usr/bin/env bash
set -euo pipefail

target="${1:?usage: compile-desktop.sh <target>}"
configs=(--config src-tauri/tauri.ci.conf.json)
cargo_args=(--timings --config profile.release.package.iyw-claw.opt-level=1)
case "$target" in
  x86_64-apple-darwin|aarch64-apple-darwin|aarch64-unknown-linux-gnu)
    # 限制同机编译压力；仅覆盖应用 package，不改变 worker 和依赖的缓存身份。
    export CARGO_BUILD_JOBS=2
    cargo_args+=(--config profile.release.package.iyw-claw.codegen-units=16)
    ;;
  x86_64-unknown-linux-gnu) ;;
  *) echo "::error::Unsupported parallel desktop target: $target"; exit 1 ;;
esac
if [[ "$target" == "aarch64-unknown-linux-gnu" ]]; then
  configs+=(--config src-tauri/tauri.linux-arm64-ci.conf.json)
fi

readonly SAMPLE_INTERVAL_SECONDS=60
readonly PROCESS_SAMPLE_COUNT=8
readonly KERNEL_SAMPLE_COUNT=30
readonly report="${RUNNER_TEMP:?}/desktop-build-${target}.log"
monitor_pid=""

snapshot() {
  echo "[build-resources] $(date -u '+%Y-%m-%dT%H:%M:%SZ')"
  if [[ "$OSTYPE" == darwin* ]]; then
    sysctl hw.memsize vm.swapusage || true
    vm_stat | grep -E 'page size|Pages free|Pages active|Pages wired|compressor|Pageouts|Swapins|Swapouts' || true
  else
    free -m || true
    if [[ -r /proc/pressure/memory ]]; then cat /proc/pressure/memory; fi
  fi
  # 只输出进程名，不输出可能携带凭证的命令参数。
  ps -axo pid,ppid,pcpu,rss,comm | sort -k4,4nr | head -n "$PROCESS_SAMPLE_COUNT" || true
}

monitor() {
  local sleeper=""
  trap 'if [[ -n "$sleeper" ]]; then kill "$sleeper" 2>/dev/null || true; wait "$sleeper" 2>/dev/null || true; fi; exit 0' TERM INT
  while true; do
    snapshot | tee -a "$report" || true
    sleep "$SAMPLE_INTERVAL_SECONDS" &
    sleeper=$!
    wait "$sleeper" || true
  done
}

finish() {
  local status=$?
  trap - EXIT
  if [[ -n "$monitor_pid" ]]; then
    kill "$monitor_pid" 2>/dev/null || true
    wait "$monitor_pid" 2>/dev/null || true
  fi
  snapshot | tee -a "$report" || true
  if [[ "$OSTYPE" == linux* && "$status" -ne 0 ]]; then
    { dmesg 2>&1 | grep -Ei 'out of memory|oom-kill|killed process|permission denied|operation not permitted' | tail -n "$KERNEL_SAMPLE_COUNT"; } >> "$report" || true
  fi
  echo "[desktop-build] target=$target exit=$status" | tee -a "$report" || true
  exit "$status"
}

trap finish EXIT
trap 'exit 130' INT
trap 'exit 143' TERM
echo "[desktop-build] target=$target jobs=${CARGO_BUILD_JOBS:-default} cargo_args=${cargo_args[*]}" | tee "$report"
df -h . >> "$report" || true
monitor &
monitor_pid=$!
time_args=(-v)
if [[ "$OSTYPE" == darwin* ]]; then time_args=(-l); fi
/usr/bin/time "${time_args[@]}" pnpm dlx @tauri-apps/cli@2.11.4 build \
  --target "$target" --features tauri-runtime "${configs[@]}" \
  --config src-tauri/tauri.compile-only.conf.json --no-bundle --no-sign \
  -- "${cargo_args[@]}"
