#!/usr/bin/env bash
#
# iyw-claw Server installer
# Usage:
#   curl -fsSL https://raw.githubusercontent.com/hwzlikewyh/iyw-claw/v0.1.93/install.sh | bash -s -- --version v0.1.93
#

set -euo pipefail

REPO="hwzlikewyh/iyw-claw"
INSTALL_DIR="${IYW_CLAW_INSTALL_DIR:-/usr/local/bin}"
WEB_DIR="${IYW_CLAW_WEB_DIR:-/usr/local/share/iyw-claw/web}"
VERSION=""
MIN_HTTP_ONLY_VERSION="0.1.93"
MINISIGN_PUBLIC_KEY="RWQs3MShTUgMUqJIgj5NzBI/EZyDJcjPnIGgzNUuBvd21qtV152OjF9X"
STABLE_TAG_RE='^v(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)$'
# 限制下载体积和归档声明的解压规模，避免异常资产耗尽磁盘或内存。
MAX_ARCHIVE_BYTES=268435456
MAX_ARCHIVE_ENTRIES=20000
MAX_EXTRACTED_BYTES=1073741824
# Stale iyw-claw-server binaries elsewhere in PATH are removed by default so
# the user's command always runs the freshly installed binary. Set
# IYW_CLAW_NO_CLEANUP=1 (or pass --no-cleanup) to disable.
CLEANUP_CONFLICTS=1
if [ "${IYW_CLAW_NO_CLEANUP:-0}" = "1" ]; then
  CLEANUP_CONFLICTS=0
fi

# The server owns the built-in MCP endpoint over Streamable HTTP, so it is the
# only executable managed by this installer.
MANAGED_BINS=(iyw-claw-server)

# Legacy built-in MCP companions are not managed binaries anymore. Remove only
# the exact old filenames from INSTALL_DIR.
LEGACY_MCP_NAME_RE='^iyw-claw-mcp(-(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)(-(0|[1-9][0-9]*|[0-9]*[A-Za-z-][0-9A-Za-z-]*)(\.(0|[1-9][0-9]*|[0-9]*[A-Za-z-][0-9A-Za-z-]*))*)?(\+[0-9A-Za-z-]+(\.[0-9A-Za-z-]+)*)?)?(\.exe)?$'

# ── Parse arguments ──

while [[ $# -gt 0 ]]; do
  case "$1" in
    --version)    VERSION="$2"; shift 2 ;;
    --dir)        INSTALL_DIR="$2"; shift 2 ;;
    --no-cleanup) CLEANUP_CONFLICTS=0; shift ;;
    --help)
      echo "Usage: install.sh [--version VERSION] [--dir INSTALL_DIR] [--no-cleanup]"
      echo ""
      echo "Options:"
      echo "  --version     Stable HTTP-only version (v0.1.93 or newer). Default: latest"
      echo "  --dir         Installation directory. Default: /usr/local/bin"
      echo "  --no-cleanup  Keep stale iyw-claw-server binaries found elsewhere in PATH"
      echo "                (default: remove them so the new install is what runs)"
      echo ""
      echo "Environment:"
      echo "  IYW_CLAW_INSTALL_DIR  Same as --dir"
      echo "  IYW_CLAW_WEB_DIR      Web asset directory"
      echo "  IYW_CLAW_NO_CLEANUP   Set to 1 to behave like --no-cleanup"
      echo ""
      echo "Requires: curl, minisign, tar, procps, and GNU coreutils"
      exit 0
      ;;
    *) echo "Unknown option: $1"; exit 1 ;;
  esac
done

# ── Detect platform ──

OS="$(uname -s)"
ARCH="$(uname -m)"

case "$OS" in
  Linux) PLATFORM="linux" ;;
  Darwin)
    echo "Error: macOS server archives are not published by the current release workflow."
    echo "       Refusing to construct an unavailable or unsigned migration path."
    exit 1
    ;;
  *) echo "Error: unsupported OS: $OS"; exit 1 ;;
esac

case "$ARCH" in
  x86_64|amd64)  ARCH_SUFFIX="x64" ;;
  aarch64|arm64)  ARCH_SUFFIX="arm64" ;;
  *)              echo "Error: unsupported architecture: $ARCH"; exit 1 ;;
esac

ARTIFACT="iyw-claw-server-${PLATFORM}-${ARCH_SUFFIX}"

# ── Resolve version ──

if ! command -v curl >/dev/null 2>&1; then
  echo "Error: required command 'curl' is not available." >&2
  exit 1
fi
if [ -z "$VERSION" ]; then
  echo "Fetching latest release..."
  VERSION=$(curl -fsSL "https://api.github.com/repos/${REPO}/releases/latest" \
    | grep '"tag_name"' | head -1 | cut -d'"' -f4)
  if [ -z "$VERSION" ]; then
    echo "Error: could not determine latest version"
    exit 1
  fi
fi

if [[ ! "$VERSION" =~ $STABLE_TAG_RE ]]; then
  echo "Error: release tag '${VERSION}' must be stable SemVer in the form vMAJOR.MINOR.PATCH." >&2
  exit 1
fi
TARGET_VER="${VERSION#v}"
IFS='.' read -r TARGET_MAJOR TARGET_MINOR TARGET_PATCH <<< "$TARGET_VER"
if (( TARGET_MAJOR == 0 && TARGET_MINOR < 1 )) \
   || (( TARGET_MAJOR == 0 && TARGET_MINOR == 1 && TARGET_PATCH < 93 )); then
  echo "Error: ${VERSION} predates the HTTP-only built-in MCP release." >&2
  echo "       Install ${MIN_HTTP_ONLY_VERSION} or newer; the existing installation was not changed." >&2
  exit 1
fi
for _required_command in base64 minisign tar realpath find awk sort timeout pgrep readlink; do
  if ! command -v "$_required_command" >/dev/null 2>&1; then
    echo "Error: required command '$_required_command' is not available." >&2
    echo "       No process or installed file was changed." >&2
    exit 1
  fi
done

# ── Helpers ──

# Canonicalize a path (resolve symlinks). Falls back to the input if no tool available.
canon_path() {
  local p="$1"
  [ -z "$p" ] && return 0
  if command -v readlink >/dev/null 2>&1 && readlink -f / >/dev/null 2>&1; then
    readlink -f "$p" 2>/dev/null || echo "$p"
  elif command -v realpath >/dev/null 2>&1; then
    realpath "$p" 2>/dev/null || echo "$p"
  else
    echo "$p"
  fi
}

# Read the version of a iyw-claw-server binary (with a 3s timeout for old binaries
# that lack --version support and would otherwise start the full server).
read_bin_version() {
  local bin="$1" output
  [ -x "$bin" ] || return 0
  if output="$(timeout --signal=TERM --kill-after=1 3 "$bin" --version 2>/dev/null)"; then
    printf '%s\n' "$output" | head -1 | tr -d '[:space:]'
  fi
}

decode_tauri_signature() {
  local source="$1" destination="$2"
  [ -s "$source" ] || return 1
  [ "$(wc -c < "$source")" -le 16384 ] || return 1
  base64 --decode "$source" > "$destination" 2>/dev/null
  [ -s "$destination" ]
}

validate_archive_limits() {
  local archive="$1" status
  [ "$(wc -c < "$archive")" -le "$MAX_ARCHIVE_BYTES" ] || {
    echo "Error: release archive exceeds ${MAX_ARCHIVE_BYTES} bytes." >&2
    return 1
  }
  tar -tvzf "$archive" | awk \
    -v max_entries="$MAX_ARCHIVE_ENTRIES" \
    -v max_bytes="$MAX_EXTRACTED_BYTES" '
      $3 !~ /^[0-9]+$/ { exit 3 }
      { entries += 1; bytes += $3 }
      entries > max_entries || bytes > max_bytes { exit 2 }
      END { if (NR == 0) exit 4 }
    ' && return 0
  status=$?
  case "$status" in
    2) echo "Error: archive extraction limits would be exceeded." >&2 ;;
    3) echo "Error: archive size metadata is not parseable." >&2 ;;
    4) echo "Error: release archive is empty." >&2 ;;
    *) echo "Error: could not inspect archive extraction limits." >&2 ;;
  esac
  return 1
}

validate_archive_inventory() {
  local archive="$1" entries entry normalized normalized_lower duplicate
  entries="$(tar -tzf "$archive")" || return 1
  [ -n "$entries" ] || return 1
  duplicate="$(printf '%s\n' "$entries" | sed 's:/*$::' | sort | uniq -d | head -1)"
  [ -z "$duplicate" ] || {
    echo "Error: duplicate archive entry: $duplicate" >&2
    return 1
  }
  while IFS= read -r entry; do
    normalized="${entry%/}"
    normalized_lower="$(printf '%s' "$normalized" | LC_ALL=C tr '[:upper:]' '[:lower:]')"
    case "$normalized_lower" in
      "$ARTIFACT"|"$ARTIFACT/iyw-claw-server"|"$ARTIFACT/web"|"$ARTIFACT/web/"*) ;;
      *) echo "Error: unexpected archive entry: $entry" >&2; return 1 ;;
    esac
    case "/$normalized/" in
      *"/../"*|*"/./"*) echo "Error: unsafe archive path: $entry" >&2; return 1 ;;
    esac
    case "$normalized" in
      /*|*\\*|*:* ) echo "Error: unsafe archive path: $entry" >&2; return 1 ;;
    esac
    if printf '%s\n' "$normalized" | tr '/' '\n' \
      | LC_ALL=C grep -Eiq '^iyw-claw-mcp'; then
      echo "Error: legacy MCP content is forbidden in HTTP-only archives: $entry" >&2
      return 1
    fi
  done <<< "$entries"
  if tar -tvzf "$archive" | awk '$1 !~ /^[-d]/ { exit 1 }'; then
    return 0
  fi
  echo "Error: archive contains a link or special filesystem entry." >&2
  return 1
}

assert_bundle_inventory() {
  local root="$1" path leaf
  [ -d "$root" ] && [ ! -L "$root" ] || return 1
  for path in "$root"/* "$root"/.[!.]* "$root"/..?*; do
    [ -e "$path" ] || [ -L "$path" ] || continue
    leaf="$(basename "$path")"
    case "$leaf" in iyw-claw-server|web) ;; *) return 1 ;; esac
  done
  if find "$root" \( -type l -o -type f -iname 'iyw-claw-mcp*' \) -print | grep -q .; then
    return 1
  fi
  [ -f "$root/iyw-claw-server" ] && [ ! -L "$root/iyw-claw-server" ] \
    && [ -s "$root/iyw-claw-server" ] && [ -x "$root/iyw-claw-server" ] \
    && [ -f "$root/web/index.html" ] && [ ! -L "$root/web/index.html" ] \
    && [ -s "$root/web/index.html" ]
}

legacy_mcp_paths() {
  local path name
  [ -e "$INSTALL_DIR" ] || [ -L "$INSTALL_DIR" ] || return 0
  if [ ! -d "$INSTALL_DIR" ] || [ ! -r "$INSTALL_DIR" ] || [ ! -x "$INSTALL_DIR" ]; then
    echo "Error: cannot inspect legacy MCP files in ${INSTALL_DIR}." >&2
    return 1
  fi
  for path in "${INSTALL_DIR}"/*; do
    [ -e "$path" ] || [ -L "$path" ] || continue
    name="$(basename "$path")"
    if is_legacy_mcp_name "$name"; then
      if [ -L "$path" ] || [ ! -f "$path" ]; then
        echo "Error: legacy MCP candidate is not a regular file: $path" >&2
        return 1
      fi
      printf '%s\n' "$path"
    fi
  done
  return 0
}

is_legacy_mcp_name() {
  local normalized
  normalized="$(printf '%s' "$1" | LC_ALL=C tr '[:upper:]' '[:lower:]')"
  [[ "$normalized" =~ $LEGACY_MCP_NAME_RE ]]
}

candidate_process_pids() {
  local kind="$1" name_pattern pattern candidates status
  if [ "$kind" = "server" ]; then
    pattern="iyw-claw-server"
    candidates="$(LC_ALL=C pgrep -x "$pattern" 2>/dev/null)" && status=0 || status=$?
  else
    name_pattern="${LEGACY_MCP_NAME_RE#^}"
    name_pattern="${name_pattern%\$}"
    pattern="(^|/)${name_pattern}([[:space:]]|$)"
    candidates="$(LC_ALL=C pgrep -if "$pattern" 2>/dev/null)" && status=0 || status=$?
  fi
  if [ "$status" -eq 0 ]; then
    printf '%s\n' "$candidates"
    return 0
  fi
  [ "$status" -eq 1 ] && return 0
  echo "Error: failed to enumerate ${kind} process candidates." >&2
  return 1
}

canonical_existing_dir() {
  local directory="$1" resolved
  [ -d "$directory" ] || return 1
  resolved="$(cd -P -- "$directory" 2>/dev/null && pwd -P)" || return 1
  [ -n "$resolved" ] || return 1
  printf '%s\n' "$resolved"
}

process_executable_path() {
  local pid="$1" executable details line
  if [ "$OS" = "Linux" ]; then
    executable="$(readlink "/proc/${pid}/exe" 2>/dev/null)" || return 1
    executable="${executable% (deleted)}"
  else
    command -v lsof >/dev/null 2>&1 || return 1
    details="$(lsof -a -p "$pid" -d txt -Fn 2>/dev/null)" || return 1
    while IFS= read -r line; do
      case "$line" in
        n/*)
          executable="${line#n}"
          break
          ;;
      esac
    done <<< "$details"
  fi
  [ -n "$executable" ] || return 1
  case "$executable" in /*) ;; *) return 1 ;; esac
  printf '%s\n' "$executable"
}

process_start_time() {
  local pid="$1" stat rest start_time
  IFS= read -r stat < "/proc/${pid}/stat" || return 1
  case "$stat" in *") "*) ;; *) return 1 ;; esac
  rest="${stat##*) }"
  set -- $rest
  [ "$#" -ge 20 ] || return 1
  start_time="${20}"
  [[ "$start_time" =~ ^[0-9]+$ ]] || return 1
  printf '%s\n' "$start_time"
}

process_owner_uid() {
  local pid="$1" owner
  owner="$(LC_ALL=C awk '/^Uid:/ { print $2; exit }' "/proc/${pid}/status" 2>/dev/null)" || return 1
  [[ "$owner" =~ ^[0-9]+$ ]] || return 1
  printf '%s\n' "$owner"
}

# 用 start time 包住 owner/executable 读取，拒绝读取期间发生的 PID 复用。
read_process_identity() {
  local pid="$1" start_before start_after owner executable
  start_before="$(process_start_time "$pid")" || return 1
  owner="$(process_owner_uid "$pid")" || return 1
  executable="$(process_executable_path "$pid")" || return 1
  executable="$(realpath --canonicalize-missing -- "$executable")" || return 1
  start_after="$(process_start_time "$pid")" || return 1
  [ "$start_before" = "$start_after" ] || return 1
  case "$executable" in *'|'*|*$'\n'*) return 1 ;; esac
  printf '%s|%s|%s|%s\n' "$pid" "$start_before" "$owner" "$executable"
}

scoped_process_snapshots() {
  local kind="$1" target candidates current_uid pid identity owner executable parent name
  [ -e "$INSTALL_DIR" ] || [ -L "$INSTALL_DIR" ] || return 0
  target="$(canonical_existing_dir "$INSTALL_DIR")" || {
    echo "Error: cannot resolve install directory ${INSTALL_DIR}." >&2
    return 1
  }
  current_uid="$(id -u 2>/dev/null)" || current_uid=""
  [[ "$current_uid" =~ ^[0-9]+$ ]] || {
    echo "Error: cannot resolve the current user id." >&2
    return 1
  }
  candidates="$(candidate_process_pids "$kind")" || return 1
  while IFS= read -r pid; do
    [ -n "$pid" ] || continue
    [[ "$pid" =~ ^[0-9]+$ ]] || {
      echo "Error: invalid ${kind} process id: ${pid}." >&2
      return 1
    }
    identity="$(read_process_identity "$pid")" || {
      echo "Error: cannot read stable identity for ${kind} process ${pid}." >&2
      return 1
    }
    IFS='|' read -r _ _ owner executable <<< "$identity"
    parent="$(canonical_existing_dir "$(dirname "$executable")")" || {
      echo "Error: cannot resolve executable parent for ${kind} process ${pid}." >&2
      return 1
    }
    name="$(basename "$executable")" || {
      echo "Error: cannot parse executable name for ${kind} process ${pid}." >&2
      return 1
    }
    [ -n "$name" ] || {
      echo "Error: executable name is empty for ${kind} process ${pid}." >&2
      return 1
    }
    if [ "$kind" = "server" ]; then
      [ "$name" = "iyw-claw-server" ] || continue
    else
      is_legacy_mcp_name "$name" || continue
    fi
    [ "$parent" = "$target" ] || continue
    if [ "$owner" != "$current_uid" ]; then
      echo "Error: refusing to signal ${kind} process ${pid} owned by uid ${owner}." >&2
      return 1
    fi
    printf '%s\n' "$identity"
  done <<< "$candidates"
}

assert_process_identity() {
  local expected="$1" pid current
  IFS='|' read -r pid _ _ _ <<< "$expected"
  current="$(read_process_identity "$pid")" || return 1
  [ "$current" = "$expected" ]
}

signal_process_snapshots() {
  local snapshots="$1" signal="$2" label="$3" snapshot pid
  while IFS= read -r snapshot; do
    [ -n "$snapshot" ] || continue
    IFS='|' read -r pid _ _ _ <<< "$snapshot"
    if ! assert_process_identity "$snapshot"; then
      echo "Error: ${label} process ${pid} identity changed before ${signal}." >&2
      return 1
    fi
    if ! kill -s "$signal" "$pid" 2>/dev/null; then
      echo "Error: failed to send ${signal} to ${label} process ${pid}." >&2
      return 1
    fi
  done <<< "$snapshots"
}

stop_scoped_processes() {
  local kind="$1" label="$2" attempts="$3" snapshots attempt
  for ((attempt = 1; attempt <= attempts; attempt += 1)); do
    snapshots="$(scoped_process_snapshots "$kind")" || return 1
    [ -z "$snapshots" ] && return 0
    echo "Stopping ${label} process(es)..."
    signal_process_snapshots "$snapshots" TERM "$label" || return 1
    sleep 1
  done
  snapshots="$(scoped_process_snapshots "$kind")" || return 1
  if [ -n "$snapshots" ]; then
    echo "Force stopping ${label}..."
    signal_process_snapshots "$snapshots" KILL "$label" || return 1
    sleep 1
  fi
  snapshots="$(scoped_process_snapshots "$kind")" || return 1
  if [ -n "$snapshots" ]; then
    echo "Error: ${label} process(es) are still running." >&2
    return 1
  fi
  return 0
}

legacy_mcp_pids() {
  local snapshots snapshot pid
  snapshots="$(scoped_process_snapshots "legacy-mcp")" || return 1
  while IFS= read -r snapshot; do
    [ -n "$snapshot" ] || continue
    IFS='|' read -r pid _ _ _ <<< "$snapshot"
    printf '%s\n' "$pid"
  done <<< "$snapshots"
}

stop_legacy_mcp_processes() {
  stop_scoped_processes "legacy-mcp" "legacy iyw-claw-mcp" 3
}

# quarantine 位于 server 事务目录，所有移动均保持在同一文件系统。
quarantine_legacy_mcp_files() {
  local path paths name destination
  if ! paths="$(legacy_mcp_paths)"; then
    return 1
  fi
  [ -z "$paths" ] && return 0
  if ! resolve_priv "$INSTALL_DIR"; then
    echo "Error: need elevated privileges to quarantine legacy MCP files." >&2
    return 1
  fi
  LEGACY_QUARANTINE_DIR="$SERVER_TXN_DIR/legacy-quarantine"
  priv_run mkdir -- "$LEGACY_QUARANTINE_DIR" || return 1
  while IFS= read -r path; do
    [ -n "$path" ] || continue
    name="$(basename "$path")"
    destination="$LEGACY_QUARANTINE_DIR/$name"
    if [ -L "$path" ] || [ ! -f "$path" ] || ! is_legacy_mcp_name "$name"; then
      echo "Error: legacy MCP file changed before quarantine: $path" >&2
      return 1
    fi
    LEGACY_QUARANTINED=1
    if ! priv_run mv -- "$path" "$destination"; then
      echo "Error: failed to quarantine legacy MCP file $path" >&2
      return 1
    fi
    if [ -z "$LEGACY_QUARANTINE_NAMES" ]; then
      LEGACY_QUARANTINE_NAMES="$name"
    else
      LEGACY_QUARANTINE_NAMES="${LEGACY_QUARANTINE_NAMES}"$'\n'"$name"
    fi
    priv_run test -f "$destination" || return 1
    echo "  quarantined legacy MCP file $path"
  done <<< "$paths"
  paths="$(legacy_mcp_paths)" || return 1
  if [ -n "$paths" ]; then
    echo "Error: legacy iyw-claw-mcp files remain in ${INSTALL_DIR}." >&2
    return 1
  fi
}

valid_legacy_quarantine() {
  [ -n "$SERVER_TXN_DIR" ] \
    && [ "$LEGACY_QUARANTINE_DIR" = "$SERVER_TXN_DIR/legacy-quarantine" ]
}

validate_legacy_quarantine_inventory() {
  local actual expected
  valid_legacy_quarantine || return 1
  resolve_priv "$INSTALL_DIR" || return 1
  priv_run test -d "$LEGACY_QUARANTINE_DIR" || return 1
  ! priv_run test -L "$LEGACY_QUARANTINE_DIR" || return 1
  actual="$(priv_run find "$LEGACY_QUARANTINE_DIR" -mindepth 1 -maxdepth 1 \
    -printf '%f\n')" || return 1
  actual="$(printf '%s\n' "$actual" | LC_ALL=C sort)"
  expected="$(printf '%s\n' "$LEGACY_QUARANTINE_NAMES" | LC_ALL=C sort)"
  [ -n "$expected" ] && [ "$actual" = "$expected" ]
}

restore_legacy_quarantine() {
  local name source destination remaining="" status=0
  [ "$LEGACY_QUARANTINED" -eq 1 ] || return 0
  validate_legacy_quarantine_inventory || return 1
  while IFS= read -r name; do
    [ -n "$name" ] || continue
    source="$LEGACY_QUARANTINE_DIR/$name"
    destination="$INSTALL_DIR/$name"
    if ! is_legacy_mcp_name "$name" \
       || ! priv_run test -f "$source" \
       || priv_run test -L "$source" \
       || priv_run test -e "$destination" \
       || priv_run test -L "$destination" \
       || ! priv_run mv -- "$source" "$destination"; then
      status=1
      if [ -z "$remaining" ]; then
        remaining="$name"
      else
        remaining="${remaining}"$'\n'"$name"
      fi
    fi
  done <<< "$LEGACY_QUARANTINE_NAMES"
  LEGACY_QUARANTINE_NAMES="$remaining"
  [ "$status" -eq 0 ] || return 1
  priv_run rmdir -- "$LEGACY_QUARANTINE_DIR" || return 1
  LEGACY_QUARANTINED=0
  LEGACY_QUARANTINE_DIR=""
}

remove_legacy_quarantine() {
  [ "$LEGACY_QUARANTINED" -eq 1 ] || return 0
  if ! validate_legacy_quarantine_inventory; then
    LEGACY_QUARANTINE_CLEANUP_FAILED=1
    echo "Error: legacy MCP quarantine changed before cleanup." >&2
    echo "       Recovery data remains at ${LEGACY_QUARANTINE_DIR}." >&2
    return 1
  fi
  if ! priv_run rm -rf -- "$LEGACY_QUARANTINE_DIR" \
     || priv_run test -e "$LEGACY_QUARANTINE_DIR"; then
    LEGACY_QUARANTINE_CLEANUP_FAILED=1
    echo "Error: migration cleanup could not delete ${LEGACY_QUARANTINE_DIR}." >&2
    echo "       The verified HTTP-only bundle remains installed; cleanup is incomplete." >&2
    return 1
  fi
  LEGACY_QUARANTINED=0
  LEGACY_QUARANTINE_NAMES=""
  LEGACY_QUARANTINE_DIR=""
}

# ── Privilege model ──
#
# Root can write anywhere and must NEVER call `sudo`: minimal root environments
# (containers, slim images) frequently don't ship sudo, and a blind `sudo mkdir`
# there aborts the whole script under `set -e` AFTER the binaries already landed
# — leaving a half-installed tree the version short-circuit then refuses to
# repair. A non-root user needs sudo only when the destination's nearest
# existing ancestor isn't writable.

PRIV=""
IS_ROOT=0
# Conservative default: if `id -u` somehow fails, assume NON-root (echo 1) so we
# fall back to writability-probing + sudo rather than wrongly skipping elevation.
# This is still correct for a real root whose `id` broke: `[ -w ]` on existing
# system dirs is true for root, so resolve_priv runs directly anyway.
if [ "$(id -u 2>/dev/null || echo 1)" = "0" ]; then
  IS_ROOT=1
fi

HAVE_SUDO=0
if command -v sudo >/dev/null 2>&1; then
  HAVE_SUDO=1
fi

# Walk up from $1 to the first ancestor that already exists, so writability can
# be tested for a not-yet-created path (e.g. /usr/local/share/iyw-claw/web, whose
# parent /usr/local/share/iyw-claw also doesn't exist on a fresh install).
nearest_existing_ancestor() {
  local p="$1"
  while [ -n "$p" ] && [ "$p" != "/" ] && [ ! -e "$p" ]; do
    p="$(dirname "$p")"
  done
  echo "$p"
}

# Decide how to create/write into directory $1. Sets global PRIV to "" (run
# directly) or "sudo". Returns non-zero — without aborting under `set -e`, since
# callers invoke it via `if` — when elevation is required but sudo is absent.
resolve_priv() {
  PRIV=""
  [ "$IS_ROOT" -eq 1 ] && return 0
  local anchor
  anchor="$(nearest_existing_ancestor "$1")"
  [ -w "$anchor" ] && return 0
  if [ "$HAVE_SUDO" -eq 1 ]; then
    PRIV="sudo"
    return 0
  fi
  return 1
}

# Run "$@", elevating with sudo only when the last resolve_priv call decided so.
priv_run() {
  if [ -n "$PRIV" ]; then
    sudo "$@"
  else
    "$@"
  fi
}

safe_lexical_path() {
  realpath --canonicalize-missing --no-symlinks -- "$1"
}

assert_path_chain_safe() {
  local input="$1" expected="$2" normalized cursor part
  normalized="$(safe_lexical_path "$input")" || return 1
  cursor=""
  IFS='/' read -ra _SAFE_PARTS <<< "${normalized#/}"
  for part in "${_SAFE_PARTS[@]}"; do
    [ -n "$part" ] || continue
    cursor="${cursor}/${part}"
    if [ -L "$cursor" ]; then
      echo "Error: target path contains a symbolic link: $cursor" >&2
      return 1
    fi
  done
  if [ -e "$normalized" ]; then
    case "$expected" in
      dir) [ -d "$normalized" ] || return 1 ;;
      file) [ -f "$normalized" ] || return 1 ;;
      any) ;;
      *) return 1 ;;
    esac
  fi
  printf '%s\n' "$normalized"
}

validate_target_layout() {
  local install web destination
  install="$(assert_path_chain_safe "$INSTALL_DIR" dir)" || return 1
  web="$(assert_path_chain_safe "$WEB_DIR" dir)" || return 1
  destination="$(assert_path_chain_safe "$install/iyw-claw-server" file)" || return 1
  if [ "$install" = "$web" ]; then
    echo "Error: web directory must not equal the binary installation directory." >&2
    return 1
  fi
  case "$install/" in
    "$web/"*)
      echo "Error: web directory must not contain the binary installation directory." >&2
      return 1
      ;;
  esac
  INSTALL_DIR="$install"
  WEB_DIR="$web"
  DEST_BIN="$destination"
}

prepare_target_staging() {
  local install_version web_parent
  resolve_priv "$INSTALL_DIR" || return 1
  priv_run mkdir -p -- "$INSTALL_DIR"
  SERVER_TXN_DIR="$(priv_run mktemp -d "$INSTALL_DIR/.iyw-claw-install.XXXXXX")" || return 1
  priv_run cp -- "$BUNDLE_ROOT/iyw-claw-server" "$SERVER_TXN_DIR/new-server"
  priv_run chmod 0755 "$SERVER_TXN_DIR/new-server"
  install_version="$(priv_run "$SERVER_TXN_DIR/new-server" --version 2>/dev/null)" || return 1
  [ "$install_version" = "$TARGET_VER" ] || {
    echo "Error: staged server version is ${install_version:-missing}; expected $TARGET_VER." >&2
    return 1
  }

  web_parent="$(dirname "$WEB_DIR")"
  resolve_priv "$web_parent" || return 1
  priv_run mkdir -p -- "$web_parent"
  WEB_TXN_DIR="$(priv_run mktemp -d "$web_parent/.iyw-claw-web-install.XXXXXX")" || return 1
  priv_run mkdir -- "$WEB_TXN_DIR/new-web"
  priv_run cp -R -- "$BUNDLE_ROOT/web/." "$WEB_TXN_DIR/new-web/"
  priv_run test -s "$WEB_TXN_DIR/new-web/index.html"
}

rollback_web_swap() {
  local status=0
  resolve_priv "$(dirname "$WEB_DIR")" || return 1
  if [ "$WEB_SWAPPED" -eq 1 ]; then
    if priv_run rm -rf -- "$WEB_DIR"; then
      WEB_SWAPPED=0
    else
      status=1
    fi
  fi
  if [ "$status" -eq 0 ] && [ "$WEB_BACKED_UP" -eq 1 ]; then
    if priv_run test -e "$WEB_DIR" \
       || ! priv_run test -e "$WEB_TXN_DIR/old-web" \
       || ! priv_run mv -- "$WEB_TXN_DIR/old-web" "$WEB_DIR"; then
      status=1
    else
      WEB_BACKED_UP=0
    fi
  fi
  return "$status"
}

rollback_server_swap() {
  local status=0
  resolve_priv "$INSTALL_DIR" || return 1
  if [ "$SERVER_SWAPPED" -eq 1 ]; then
    if priv_run rm -f -- "$DEST_BIN"; then
      SERVER_SWAPPED=0
    else
      status=1
    fi
  fi
  if [ "$status" -eq 0 ] && [ "$SERVER_BACKED_UP" -eq 1 ]; then
    if priv_run test -e "$DEST_BIN" \
       || ! priv_run test -e "$SERVER_TXN_DIR/old-server" \
       || ! priv_run mv -- "$SERVER_TXN_DIR/old-server" "$DEST_BIN"; then
      status=1
    else
      SERVER_BACKED_UP=0
    fi
  fi
  return "$status"
}

rollback_install_transaction() {
  local status=0
  rollback_web_swap || status=1
  rollback_server_swap || status=1
  restore_legacy_quarantine || status=1
  return "$status"
}

swap_server_bundle() {
  resolve_priv "$INSTALL_DIR" || return 1
  if priv_run test -e "$DEST_BIN"; then
    priv_run mv -- "$DEST_BIN" "$SERVER_TXN_DIR/old-server" || return 1
    SERVER_BACKED_UP=1
  fi
  priv_run mv -- "$SERVER_TXN_DIR/new-server" "$DEST_BIN" || return 1
  SERVER_SWAPPED=1
}

swap_web_bundle() {
  resolve_priv "$(dirname "$WEB_DIR")" || return 1
  if priv_run test -e "$WEB_DIR"; then
    priv_run mv -- "$WEB_DIR" "$WEB_TXN_DIR/old-web" || return 1
    WEB_BACKED_UP=1
  fi
  priv_run mv -- "$WEB_TXN_DIR/new-web" "$WEB_DIR" || return 1
  WEB_SWAPPED=1
}

commit_staged_bundle() {
  swap_server_bundle || return 1
  swap_web_bundle
}

# ── Scan PATH for iyw-claw-server binaries that shadow the target install ──
#
# A binary "shadows" the install only if it appears in PATH BEFORE the
# destination directory: that's the binary `command -v iyw-claw-server` would
# return after install. Walk PATH and stop at the destination directory —
# anything past it cannot affect resolution today, so we leave it alone.

if ! validate_target_layout; then
  echo "Error: installation target layout is unsafe or has an invalid type." >&2
  exit 1
fi
DEST_BIN_REAL="$(canon_path "$DEST_BIN")"
INSTALL_DIR_REAL="$(canon_path "$INSTALL_DIR")"

# Scan PATH for managed binaries that shadow the destination.
PATH_CONFLICTS=()
DEST_IN_PATH=0
_SEEN_REAL=":"
IFS=':' read -ra _PATH_DIRS <<< "${PATH:-}"
for _dir in "${_PATH_DIRS[@]}"; do
  [ -z "$_dir" ] && continue
  # Match by canonical path string so the destination is recognized even when
  # the directory doesn't exist yet (e.g. first install into a fresh prefix).
  if [ "$(canon_path "$_dir")" = "$INSTALL_DIR_REAL" ]; then
    DEST_IN_PATH=1
    break
  fi
  for _name in "${MANAGED_BINS[@]}"; do
    _bin="$_dir/$_name"
    if [ -f "$_bin" ] && [ -x "$_bin" ]; then
      _real="$(canon_path "$_bin")"
      case "$_SEEN_REAL" in
        *":$_real:"*) continue ;;
      esac
      _SEEN_REAL="${_SEEN_REAL}${_real}:"
      PATH_CONFLICTS+=("$_bin")
    fi
  done
done

# If the destination directory isn't on PATH, nothing "shadows" the install —
# the new binary just won't be reachable as `iyw-claw-server`. Drop any collected
# entries; the post-install check will tell the user to fix PATH instead.
if [ "$DEST_IN_PATH" -eq 0 ]; then
  PATH_CONFLICTS=()
fi

# What does `iyw-claw-server` actually resolve to right now in PATH?
ACTIVE_BIN=""
if command -v iyw-claw-server >/dev/null 2>&1; then
  ACTIVE_BIN="$(command -v iyw-claw-server)"
fi

# ── Version detection — prefer the binary the user actually invokes ──

VERSION_CHECK_BIN=""
if [ -n "$ACTIVE_BIN" ] && [ -x "$ACTIVE_BIN" ]; then
  VERSION_CHECK_BIN="$ACTIVE_BIN"
elif [ -x "$DEST_BIN" ]; then
  VERSION_CHECK_BIN="$DEST_BIN"
fi

CURRENT_VERSION=""
if [ -n "$VERSION_CHECK_BIN" ]; then
  CURRENT_VERSION="$(read_bin_version "$VERSION_CHECK_BIN")"
fi

LEGACY_CLEANUP_REQUIRED=0
if ! LEGACY_MCP_PATHS="$(legacy_mcp_paths)"; then
  exit 1
fi
if ! LEGACY_MCP_PIDS="$(legacy_mcp_pids)"; then
  exit 1
fi
if [ -n "$LEGACY_MCP_PATHS" ] || [ -n "$LEGACY_MCP_PIDS" ]; then
  LEGACY_CLEANUP_REQUIRED=1
fi

# Only short-circuit when the active binary is up to date AND the destination
# has it AND no other PATH entries shadow it AND the web assets are present.
# The web-asset check makes the installer self-healing: a prior run that placed
# the binary but failed before copying web/ (the classic root-without-sudo
# case) is repaired on re-run instead of exiting "nothing to do" forever.
if [ -n "$CURRENT_VERSION" ] && [ "$CURRENT_VERSION" = "$TARGET_VER" ] \
   && [ "${#PATH_CONFLICTS[@]}" -eq 0 ] \
   && [ -f "$DEST_BIN" ] \
   && [ -s "$DEST_BIN" ] \
   && [ -x "$DEST_BIN" ] \
   && [ "$LEGACY_CLEANUP_REQUIRED" -eq 0 ] \
   && [ -f "${WEB_DIR}/index.html" ] \
   && [ -s "${WEB_DIR}/index.html" ]; then
  echo "iyw-claw-server is already at version ${TARGET_VER} with web assets in place, nothing to do."
  exit 0
fi

if [ -n "$CURRENT_VERSION" ] && [ "$CURRENT_VERSION" = "$TARGET_VER" ]; then
  echo "iyw-claw-server is already at ${TARGET_VER}; reinstalling to repair the existing install..."
elif [ -n "$CURRENT_VERSION" ]; then
  echo "Upgrading iyw-claw-server: ${CURRENT_VERSION} -> ${TARGET_VER}..."
else
  echo "Installing iyw-claw-server ${VERSION} (${PLATFORM}/${ARCH_SUFFIX})..."
fi

# ── Warn about iyw-claw-server binaries shadowing the target install ──

if [ "${#PATH_CONFLICTS[@]}" -gt 0 ]; then
  echo ""
  echo "Found other iyw-claw-server binaries in PATH that may shadow ${DEST_BIN}:"
  for _c in "${PATH_CONFLICTS[@]}"; do
    _cv="$(read_bin_version "$_c" 2>/dev/null || true)"
    if [ -n "$_cv" ]; then
      echo "  - $_c  (version ${_cv})"
    else
      echo "  - $_c"
    fi
  done
  if [ "$CLEANUP_CONFLICTS" = "1" ]; then
    echo "These will be removed after installation. Pass --no-cleanup to keep them."
  else
    echo "Keeping them (--no-cleanup). You may need to remove them manually so that"
    echo "typing 'iyw-claw-server' runs the new install at ${DEST_BIN}."
  fi
  echo ""
fi

# ── Download and extract ──

DOWNLOAD_URL="https://github.com/${REPO}/releases/download/${VERSION}/${ARTIFACT}.tar.gz"
SIGNATURE_URL="${DOWNLOAD_URL}.sig"
TMP_DIR="$(umask 077; mktemp -d)"
ARCHIVE_PATH="${TMP_DIR}/${ARTIFACT}.tar.gz"
TAURI_SIGNATURE_PATH="${ARCHIVE_PATH}.sig"
MINISIGN_PATH="${TMP_DIR}/${ARTIFACT}.minisig"
SERVER_TXN_DIR=""
WEB_TXN_DIR=""
SERVER_BACKED_UP=0
SERVER_SWAPPED=0
WEB_BACKED_UP=0
WEB_SWAPPED=0
LIVE_BUNDLE_VERIFIED=0
LEGACY_QUARANTINE_DIR=""
LEGACY_QUARANTINE_NAMES=""
LEGACY_QUARANTINED=0
LEGACY_QUARANTINE_CLEANUP_FAILED=0
RESTARTED_PIDS=""

preserve_server_transaction() {
  [ "$LEGACY_QUARANTINE_CLEANUP_FAILED" -eq 1 ] && return 0
  [ "$LIVE_BUNDLE_VERIFIED" -eq 0 ] || return 1
  [ "$SERVER_BACKED_UP" -eq 1 ] || [ "$SERVER_SWAPPED" -eq 1 ] \
    || [ "$LEGACY_QUARANTINED" -eq 1 ]
}

preserve_web_transaction() {
  [ "$LIVE_BUNDLE_VERIFIED" -eq 0 ] \
    && { [ "$WEB_BACKED_UP" -eq 1 ] || [ "$WEB_SWAPPED" -eq 1 ]; }
}

cleanup_transaction_dirs() {
  local status=0
  if [ -n "$SERVER_TXN_DIR" ] && ! preserve_server_transaction; then
    resolve_priv "$INSTALL_DIR" \
      && priv_run rm -rf -- "$SERVER_TXN_DIR" || status=1
  fi
  if [ -n "$WEB_TXN_DIR" ] && ! preserve_web_transaction; then
    resolve_priv "$(dirname "$WEB_DIR")" \
      && priv_run rm -rf -- "$WEB_TXN_DIR" || status=1
  fi
  return "$status"
}

report_preserved_transactions() {
  if preserve_server_transaction; then
    echo "Recovery data preserved at: ${SERVER_TXN_DIR}" >&2
  fi
  if preserve_web_transaction; then
    echo "Recovery data preserved at: ${WEB_TXN_DIR}" >&2
  fi
}

cleanup_installer() {
  local status=$? rolled_back=0
  if [ "$LIVE_BUNDLE_VERIFIED" -eq 0 ] \
     && { [ "$SERVER_BACKED_UP" -eq 1 ] || [ "$SERVER_SWAPPED" -eq 1 ] \
       || [ "$WEB_BACKED_UP" -eq 1 ] || [ "$WEB_SWAPPED" -eq 1 ] \
       || [ "$LEGACY_QUARANTINED" -eq 1 ]; }; then
    rollback_install_transaction && rolled_back=1 || status=1
  fi
  cleanup_transaction_dirs || status=1
  rm -rf -- "$TMP_DIR" || status=1
  if preserve_server_transaction || preserve_web_transaction; then
    echo "Error: automatic rollback or migration cleanup is incomplete." >&2
    report_preserved_transactions
    status=1
  elif [ "$status" -ne 0 ] && [ "$LIVE_BUNDLE_VERIFIED" -eq 0 ] \
       && [ "$rolled_back" -eq 1 ]; then
    echo "The installation transaction was rolled back; no new bundle was committed." >&2
  fi
  if [ "$status" -ne 0 ] && [ -n "$RESTARTED_PIDS" ]; then
    echo "Restart the service manually with its original environment." >&2
  fi
  trap - EXIT
  exit "$status"
}
trap cleanup_installer EXIT

echo "Downloading ${DOWNLOAD_URL}..."
if ! curl -fSL --max-filesize "$MAX_ARCHIVE_BYTES" --progress-bar \
  -o "$ARCHIVE_PATH" "$DOWNLOAD_URL"; then
  echo "Error: download failed. Check that version ${VERSION} exists and has a ${ARTIFACT} asset."
  exit 1
fi
if ! curl -fSL --max-filesize 16384 --progress-bar \
  -o "$TAURI_SIGNATURE_PATH" "$SIGNATURE_URL"; then
  echo "Error: detached signature download failed for the same fixed release tag." >&2
  exit 1
fi
if ! decode_tauri_signature "$TAURI_SIGNATURE_PATH" "$MINISIGN_PATH"; then
  echo "Error: release signature is not valid Tauri base64-wrapped minisign text." >&2
  exit 1
fi
if ! minisign -Vm "$ARCHIVE_PATH" -x "$MINISIGN_PATH" -P "$MINISIGN_PUBLIC_KEY"; then
  echo "Error: release archive signature verification failed." >&2
  exit 1
fi
if ! validate_archive_limits "$ARCHIVE_PATH"; then
  exit 1
fi
if ! validate_archive_inventory "$ARCHIVE_PATH"; then
  echo "Error: release archive violates the HTTP-only server inventory contract." >&2
  exit 1
fi

echo "Extracting..."
tar --extract --gzip --file "$ARCHIVE_PATH" --directory "$TMP_DIR" \
  --no-same-owner --no-same-permissions

BUNDLE_ROOT="${TMP_DIR}/${ARTIFACT}"
if ! assert_bundle_inventory "$BUNDLE_ROOT"; then
  echo "Error: extracted bundle is incomplete, unsafe, or contains legacy MCP content." >&2
  exit 1
fi
if ! prepare_target_staging; then
  echo "Error: could not stage and verify server/web on their target filesystems." >&2
  echo "       Check permissions and free space; the running installation was not changed." >&2
  exit 1
fi
if ! validate_target_layout; then
  echo "Error: installation target changed or became unsafe during staging." >&2
  exit 1
fi

if ! RESTARTED_PIDS="$(scoped_process_snapshots "server")"; then
  exit 1
fi
if ! stop_scoped_processes "server" "iyw-claw-server" 10; then
  exit 1
fi
if [ -n "$RESTARTED_PIDS" ]; then
  echo "iyw-claw-server stopped."
fi

if ! stop_legacy_mcp_processes; then
  exit 1
fi
if [ "$LEGACY_CLEANUP_REQUIRED" -eq 1 ]; then
  echo "Quarantining legacy iyw-claw-mcp files in the install transaction..."
fi
if ! quarantine_legacy_mcp_files; then
  echo "Error: legacy MCP quarantine failed; the bundle was not replaced." >&2
  exit 1
fi
if ! LEGACY_MCP_PIDS="$(legacy_mcp_pids)" \
   || ! LEGACY_MCP_PATHS="$(legacy_mcp_paths)"; then
  exit 1
fi
if [ -n "$LEGACY_MCP_PIDS" ] || [ -n "$LEGACY_MCP_PATHS" ]; then
  echo "Error: legacy MCP targets reappeared before bundle replacement." >&2
  exit 1
fi

if ! commit_staged_bundle; then
  echo "Error: atomic server/web replacement failed; rollback will run before exit." >&2
  exit 1
fi
if ! INSTALLED_VER="$("$DEST_BIN" --version 2>/dev/null)"; then
  echo "Error: live server --version check failed; rolling back the transaction." >&2
  exit 1
fi
if [ "$INSTALLED_VER" != "$TARGET_VER" ] \
   || [ ! -s "$DEST_BIN" ] || [ ! -x "$DEST_BIN" ] \
   || [ ! -s "$WEB_DIR/index.html" ]; then
  echo "Error: live server/web verification failed; rolling back the previous bundle." >&2
  exit 1
fi
LIVE_BUNDLE_VERIFIED=1

DEST_BIN_REAL="$(canon_path "$DEST_BIN")"

if ! remove_legacy_quarantine; then
  exit 1
fi
if ! LEGACY_MCP_PIDS="$(legacy_mcp_pids)" \
   || ! LEGACY_MCP_PATHS="$(legacy_mcp_paths)"; then
  exit 1
fi
if [ -n "$LEGACY_MCP_PIDS" ] || [ -n "$LEGACY_MCP_PATHS" ]; then
  echo "Error: legacy MCP files or processes remain; migration is incomplete." >&2
  exit 1
fi

# ── Remove shadowing binaries from earlier PATH entries ──

EXIT_STATUS=0

if [ "${#PATH_CONFLICTS[@]}" -gt 0 ] && [ "$CLEANUP_CONFLICTS" = "1" ]; then
  echo ""
  echo "Removing stale iyw-claw-server binaries..."
  for _c in "${PATH_CONFLICTS[@]}"; do
    _parent="$(dirname "$_c")"
    _rm_ok=0
    if [ -w "$_parent" ] && { [ ! -e "$_c" ] || [ -w "$_c" ]; }; then
      if rm -f "$_c" 2>/dev/null; then _rm_ok=1; fi
    else
      if sudo rm -f "$_c" 2>/dev/null; then _rm_ok=1; fi
    fi
    if [ "$_rm_ok" -eq 1 ]; then
      echo "  removed $_c"
    else
      echo "  failed to remove $_c (remove it manually so 'iyw-claw-server' resolves to the new install)"
      EXIT_STATUS=1
    fi
  done
fi

# ── Restart service if it was running ──

if [ -n "$RESTARTED_PIDS" ]; then
  echo ""
  echo "Note: iyw-claw-server was stopped for the upgrade."
  echo "Please restart it manually to ensure your environment variables (IYW_CLAW_PORT, IYW_CLAW_TOKEN, etc.) are preserved:"
  echo "  IYW_CLAW_STATIC_DIR=${WEB_DIR} iyw-claw-server"
fi

# ── Done ──

echo ""
echo "iyw-claw-server installed to ${INSTALL_DIR}/iyw-claw-server"
INSTALLED_VER=""
if ! INSTALLED_VER=$("${INSTALL_DIR}/iyw-claw-server" --version 2>/dev/null); then
  echo "Error: installed iyw-claw-server failed its --version check." >&2
  EXIT_STATUS=1
elif [ "$INSTALLED_VER" != "$TARGET_VER" ]; then
  echo "Error: installed iyw-claw-server version is ${INSTALLED_VER:-missing}; expected ${TARGET_VER}." >&2
  EXIT_STATUS=1
fi
echo "Version: ${INSTALLED_VER}"

# Verify the user's `iyw-claw-server` command actually resolves to the new binary.
ACTIVE_BIN_AFTER=""
if command -v iyw-claw-server >/dev/null 2>&1; then
  ACTIVE_BIN_AFTER="$(command -v iyw-claw-server)"
fi
ACTIVE_BIN_AFTER_REAL="$(canon_path "$ACTIVE_BIN_AFTER")"

if [ -z "$ACTIVE_BIN_AFTER" ]; then
  echo ""
  echo "Note: ${INSTALL_DIR} is not on your PATH. Add it so 'iyw-claw-server' resolves directly:"
  echo "  export PATH=\"${INSTALL_DIR}:\$PATH\""
  EXIT_STATUS=1
elif [ "$ACTIVE_BIN_AFTER_REAL" != "$DEST_BIN_REAL" ]; then
  echo ""
  echo "Warning: typing 'iyw-claw-server' still runs ${ACTIVE_BIN_AFTER}, not ${DEST_BIN}."
  echo "Another binary earlier in PATH is shadowing the new install. To fix, either:"
  echo "  - re-run without --no-cleanup (the default removes shadowing binaries), or"
  echo "  - remove the stale binary manually: rm '${ACTIVE_BIN_AFTER}', or"
  echo "  - put ${INSTALL_DIR} before its directory in PATH."
  EXIT_STATUS=1
else
  # Same path: a previous shell session may have cached the old inode.
  echo ""
  echo "Tip: if you ran iyw-claw-server earlier in this shell, run 'hash -r' (bash/zsh) to clear the path cache."
fi

echo ""
echo "Quick start:"
echo "  IYW_CLAW_STATIC_DIR=${WEB_DIR} iyw-claw-server"
echo ""
echo "Or with custom settings:"
echo "  IYW_CLAW_PORT=3080 IYW_CLAW_TOKEN=your-secret IYW_CLAW_STATIC_DIR=${WEB_DIR} iyw-claw-server"
echo ""
echo "The auth token is printed to stderr on startup if not set via IYW_CLAW_TOKEN."

exit "$EXIT_STATUS"
