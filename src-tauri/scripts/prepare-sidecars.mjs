#!/usr/bin/env node
//
// Prepare Tauri sidecars before `tauri build` / `tauri dev` consume them.
//
// What it does:
//   1. Resolves the target triple — `--target <triple>` arg, or
//      `TAURI_TARGET_TRIPLE` env, or the host's `rustc -vV` host triple.
//   2. Builds `iyw-claw-mcp` with only its dedicated Cargo feature enabled.
//      for that triple from `src-tauri/`.
//   3. Copies the produced binary to
//      `src-tauri/binaries/iyw-claw-mcp-<triple>{.exe}` so Tauri's externalBin
//      bundler picks it up under the bare name `iyw-claw-mcp` at install time.
//
// Why a separate script (not inline in beforeBuildCommand / GitHub Actions):
//   - Cross-compile in release.yml passes `--target <triple>` so we honour
//     the matrix triple rather than rebuilding for the host.
//   - Local `pnpm tauri dev` / `pnpm tauri build` invoke it without args and
//     isolate mcp-runtime artifacts from the desktop feature set.
//   - Skippable: set `IYW_CLAW_SKIP_SIDECAR=1` when iterating on the frontend
//     and you don't care about delegation.
//
// Intentionally Node-only (no shell): runs identically on macOS, Linux,
// Windows GitHub runners.

import { execFileSync } from "node:child_process"
import {
  chmodSync,
  copyFileSync,
  existsSync,
  mkdirSync,
  readFileSync,
  statSync,
} from "node:fs"
import { dirname, join, resolve } from "node:path"
import { fileURLToPath } from "node:url"
import process from "node:process"

const SCRIPT_DIR = dirname(fileURLToPath(import.meta.url))
const SRC_TAURI = resolve(SCRIPT_DIR, "..")
const BINARIES_DIR = join(SRC_TAURI, "binaries")
const BIN_NAME = "iyw-claw-mcp"
const CARGO_BIN_NAME = BIN_NAME.replaceAll("-", "_")

function log(msg) {
  console.log(`[prepare-sidecars] ${msg}`)
}

function die(msg) {
  console.error(`[prepare-sidecars][ERROR] ${msg}`)
  process.exit(1)
}

function parseArgs(argv) {
  const args = { target: null }
  for (let i = 0; i < argv.length; i++) {
    const a = argv[i]
    if (a === "--target" && argv[i + 1]) {
      args.target = argv[++i]
    } else if (a.startsWith("--target=")) {
      args.target = a.slice("--target=".length)
    }
  }
  return args
}

function resolveHostTriple() {
  try {
    const out = execFileSync("rustc", ["-vV"], { encoding: "utf8" })
    const line = out.split(/\r?\n/).find((l) => l.startsWith("host:"))
    if (!line) throw new Error("rustc -vV missing host: line")
    return line.replace(/^host:\s*/, "").trim()
  } catch (e) {
    die(`cannot determine host triple via rustc -vV: ${e.message}`)
  }
}

export function resolveBundleCompatPaths(srcTauri, target, ext) {
  const fileName = `${CARGO_BIN_NAME}${ext}`
  return [
    join(srcTauri, "target", "release", fileName),
    join(srcTauri, "target", target, "release", fileName),
  ]
}

export function resolveBuildInvocation(srcTauri, target, ext) {
  const args = [
    "build",
    "--release",
    "--bin",
    BIN_NAME,
    "--no-default-features",
    "--features",
    "mcp-runtime",
    "--target",
    target,
  ]
  return {
    args,
    built: join(srcTauri, "target", target, "release", `${BIN_NAME}${ext}`),
  }
}

export function copyFileIfChanged(source, destination) {
  if (existsSync(destination)) {
    const sourceStats = statSync(source)
    const destinationStats = statSync(destination)
    if (
      sourceStats.size === destinationStats.size &&
      readFileSync(source).equals(readFileSync(destination))
    ) {
      return false
    }
  }

  mkdirSync(dirname(destination), { recursive: true })
  copyFileSync(source, destination)
  return true
}

function main() {
  if (process.env.IYW_CLAW_SKIP_SIDECAR === "1") {
    log("IYW_CLAW_SKIP_SIDECAR=1 — skipping sidecar preparation")
    return
  }

  const { target: cliTarget } = parseArgs(process.argv.slice(2))
  const target =
    cliTarget || process.env.TAURI_TARGET_TRIPLE || resolveHostTriple()
  const isWindows = target.includes("windows")
  const ext = isWindows ? ".exe" : ""

  log(`target triple: ${target}`)
  log(
    `building ${BIN_NAME} (--release --no-default-features --features mcp-runtime)`
  )

  // cargo build needs to run from src-tauri so it resolves the local manifest
  // and shares the swatinem/rust-cache key with other cargo invocations.
  // Keep the companion free of Tauri runtime dependencies while satisfying
  // the bin's feature gate, so desktop builds do not compile it a second time.
  const build = resolveBuildInvocation(SRC_TAURI, target, ext)
  execFileSync("cargo", build.args, { stdio: "inherit", cwd: SRC_TAURI })

  const built = build.built
  if (!existsSync(built)) {
    die(`expected ${built} after cargo build, but it does not exist`)
  }

  const dest = join(BINARIES_DIR, `${BIN_NAME}-${target}${ext}`)
  const sidecarChanged = copyFileIfChanged(built, dest)
  if (!isWindows) {
    // copyFileSync preserves modes on POSIX, but be explicit for tarball
    // sources that may strip the +x bit.
    chmodSync(dest, 0o755)
  }
  log(`sidecar ${sidecarChanged ? "staged" : "unchanged"} at ${dest}`)

  // Tauri CLI 2.10 resolves Cargo target names with underscores while
  // tauri-build preserves the externalBin filename with hyphens. Stage both
  // possible Cargo output layouts so the bundler can inspect the sidecar.
  for (const compatPath of resolveBundleCompatPaths(SRC_TAURI, target, ext)) {
    const aliasChanged = copyFileIfChanged(built, compatPath)
    if (!isWindows) {
      chmodSync(compatPath, 0o755)
    }
    log(
      `bundle compatibility alias ${aliasChanged ? "staged" : "unchanged"} at ${compatPath}`
    )
  }
}

main()
