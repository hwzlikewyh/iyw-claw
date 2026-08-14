#!/usr/bin/env node
//
// Prepare the iyw-claw-mcp sidecar before `tauri build` / `tauri dev`.
//
// What it does:
//   1. Resolves the target triple — `--target <triple>` arg, or
//      `TAURI_TARGET_TRIPLE` env, or the host's `rustc -vV` host triple.
//   2. Builds `iyw-claw-mcp` with only its dedicated Cargo feature enabled
//      for that triple from `src-tauri/`.
//   3. Copies the produced binary to
//      `src-tauri/binaries/iyw-claw-mcp-<triple>{.exe}` so Tauri's externalBin
//      bundler picks it up under the bare name `iyw-claw-mcp` at install time.
//
// Why only this sidecar:
//   - `iyw-claw-mcp` is the desktop app's own companion process (same repo,
//     `mcp-runtime` feature), not an independently managed CLI. It stays in
//     the installer as an application binary.
//   - Node.js, Git, uv/uvx, codex-acp and all Agent SDK/CLI artifacts were
//     removed from the installer: they are distributed by the Fusion API
//     version center with short-lived TOS/CDN tickets and verified locally
//     before activation. `src-tauri/resources/runtime` is no longer staged.
//
// Skippable: set `IYW_CLAW_SKIP_SIDECAR=1` when iterating on the frontend
// and you don't care about delegation.
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

import { prepareAgentBrowserSidecar } from "./prepare-agent-browser-sidecar.mjs"
import { resolveSignMode, signFiles } from "./sign-windows.mjs"

const SCRIPT_DIR = dirname(fileURLToPath(import.meta.url))
const SRC_TAURI = resolve(SCRIPT_DIR, "..")
const BINARIES_DIR = join(SRC_TAURI, "binaries")
const BIN_NAME = "iyw-claw-mcp"
const CARGO_BIN_NAME = BIN_NAME.replaceAll("-", "_")
const APP_VERSION = JSON.parse(
  readFileSync(resolve(SRC_TAURI, "..", "package.json"), "utf8")
).version

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

export function resolveBundleCompatPaths(srcTauri, target, ext, hostTarget) {
  const fileName = `${CARGO_BIN_NAME}${ext}`
  const paths = [
    join(srcTauri, "target", "release", fileName),
    join(srcTauri, "target", target, "release", fileName),
  ]
  if (target === hostTarget) {
    paths.unshift(join(srcTauri, "target", "debug", `${BIN_NAME}${ext}`))
  }
  return paths
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

async function main() {
  if (process.env.IYW_CLAW_SKIP_SIDECAR === "1") {
    log("IYW_CLAW_SKIP_SIDECAR=1 — skipping sidecar preparation")
    return
  }

  const { target: cliTarget } = parseArgs(process.argv.slice(2))
  const configuredTarget = cliTarget || process.env.TAURI_TARGET_TRIPLE

  const hostTarget = resolveHostTriple()
  const target = configuredTarget || hostTarget
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
  // Statically link the MSVC CRT on Windows so the sidecar runs on machines
  // that lack the VC++ 2015–2022 redistributable.  The sidecar is a pure
  // stdio/socket binary with no DLL loading, so mixing static CRT with the
  // system DLLs it calls (kernel32, ws2_32 …) is safe and well-supported.
  const buildEnv = { ...process.env }
  if (target.includes("windows-msvc")) {
    const existing = buildEnv.RUSTFLAGS || ""
    buildEnv.RUSTFLAGS = [existing, "-C target-feature=+crt-static"]
      .filter(Boolean)
      .join(" ")
  }
  execFileSync("cargo", build.args, {
    stdio: "inherit",
    cwd: SRC_TAURI,
    env: buildEnv,
  })

  const built = build.built
  if (!existsSync(built)) {
    die(`expected ${built} after cargo build, but it does not exist`)
  }

  // Authenticode-sign the freshly built sidecar before it is copied anywhere.
  // Signing the single Cargo output means every staged copy below (binaries/
  // for externalBin plus the bundle compatibility aliases) inherits the
  // signature, so there is no way for one layout to ship unsigned.
  //
  // This has to happen here rather than in build-desktop.mjs: on the default
  // build path this script runs inside `tauri build`'s beforeBuildCommand, so
  // the outer script has no point between staging and bundling to hook.
  if (isWindows && resolveSignMode(process.env) !== "none") {
    log(`Authenticode-signing ${built}`)
    signFiles([built])
  }

  for (const bundleName of [BIN_NAME, `${BIN_NAME}-${APP_VERSION}`]) {
    const dest = join(BINARIES_DIR, `${bundleName}-${target}${ext}`)
    const sidecarChanged = copyFileIfChanged(built, dest)
    if (!isWindows) {
      // copyFileSync preserves modes on POSIX, but be explicit for tarball
      // sources that may strip the +x bit.
      chmodSync(dest, 0o755)
    }
    log(`sidecar ${sidecarChanged ? "staged" : "unchanged"} at ${dest}`)
  }

  // Tauri CLI 2.10 resolves Cargo target names with underscores while
  // tauri-build preserves the externalBin filename with hyphens. Stage both
  // possible Cargo output layouts so the bundler can inspect the sidecar.
  for (const compatPath of resolveBundleCompatPaths(
    SRC_TAURI,
    target,
    ext,
    hostTarget
  )) {
    const aliasChanged = copyFileIfChanged(built, compatPath)
    if (!isWindows) {
      chmodSync(compatPath, 0o755)
    }
    log(
      `bundle compatibility alias ${aliasChanged ? "staged" : "unchanged"} at ${compatPath}`
    )
  }

  await prepareAgentBrowserSidecar(target, buildEnv)
}

if (
  process.argv[1] &&
  resolve(process.argv[1]) === fileURLToPath(import.meta.url)
) {
  await main()
}
