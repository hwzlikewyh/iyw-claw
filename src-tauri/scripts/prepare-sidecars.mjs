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
  mkdtempSync,
  mkdirSync,
  readdirSync,
  readFileSync,
  rmSync,
  statSync,
  writeFileSync,
} from "node:fs"
import { dirname, join, resolve, win32 } from "node:path"
import { tmpdir } from "node:os"
import { createHash } from "node:crypto"
import { fileURLToPath } from "node:url"
import process from "node:process"

const SCRIPT_DIR = dirname(fileURLToPath(import.meta.url))
const SRC_TAURI = resolve(SCRIPT_DIR, "..")
const BINARIES_DIR = join(SRC_TAURI, "binaries")
const BIN_NAME = "iyw-claw-mcp"
const CARGO_BIN_NAME = BIN_NAME.replaceAll("-", "_")
const UV_VERSION = "0.8.10"
const DOWNLOAD_TIMEOUT_MS = 5 * 60 * 1000

function log(msg) {
  console.log(`[prepare-sidecars] ${msg}`)
}

function die(msg) {
  console.error(`[prepare-sidecars][ERROR] ${msg}`)
  process.exit(1)
}

function parseArgs(argv) {
  const args = { target: null, uvOnly: false }
  for (let i = 0; i < argv.length; i++) {
    const a = argv[i]
    if (a === "--target" && argv[i + 1]) {
      args.target = argv[++i]
    } else if (a.startsWith("--target=")) {
      args.target = a.slice("--target=".length)
    } else if (a === "--uv-only") {
      args.uvOnly = true
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

export function resolveUvRelease(target) {
  const platforms = {
    "aarch64-apple-darwin": ["aarch64-apple-darwin", "tar.gz"],
    "x86_64-apple-darwin": ["x86_64-apple-darwin", "tar.gz"],
    "aarch64-unknown-linux-gnu": ["aarch64-unknown-linux-gnu", "tar.gz"],
    "x86_64-unknown-linux-gnu": ["x86_64-unknown-linux-gnu", "tar.gz"],
    "aarch64-pc-windows-msvc": ["aarch64-pc-windows-msvc", "zip"],
    "i686-pc-windows-msvc": ["i686-pc-windows-msvc", "zip"],
    "x86_64-pc-windows-msvc": ["x86_64-pc-windows-msvc", "zip"],
  }
  const spec = platforms[target]
  if (!spec) die(`uv ${UV_VERSION} is not available for target ${target}`)
  const [archiveTarget, extension] = spec
  return {
    extension,
    url: `https://github.com/astral-sh/uv/releases/download/${UV_VERSION}/uv-${archiveTarget}.${extension}`,
  }
}

export function parseSha256(content) {
  const digest = content.trim().split(/\s+/)[0]?.toLowerCase()
  if (!digest || !/^[a-f0-9]{64}$/.test(digest)) {
    throw new Error("invalid uv sha256 response")
  }
  return digest
}

export function resolveExtractor(
  archive,
  destination,
  isWindows,
  windowsRoot = process.env.SystemRoot || "C:\\Windows"
) {
  return {
    command: isWindows
      ? win32.join(windowsRoot, "System32", "tar.exe")
      : "tar",
    args: ["-xf", archive, "-C", destination],
  }
}

async function download(url, label) {
  try {
    const response = await fetch(url, {
      signal: AbortSignal.timeout(DOWNLOAD_TIMEOUT_MS),
    })
    if (!response.ok) die(`${label} download failed: HTTP ${response.status}`)
    return response
  } catch (error) {
    die(`${label} download failed: ${error.message}`)
  }
}

function findFile(root, name) {
  for (const entry of readdirSync(root, { withFileTypes: true })) {
    const path = join(root, entry.name)
    if (entry.isDirectory()) {
      const found = findFile(path, name)
      if (found) return found
    } else if (entry.name === name) {
      return path
    }
  }
  return null
}

async function stageUvSidecars(target, isWindows) {
  const ext = isWindows ? ".exe" : ""
  const destinations = ["uv", "uvx"].map((name) =>
    join(BINARIES_DIR, `${name}-${target}${ext}`)
  )
  const versionMarker = join(BINARIES_DIR, `uv-${target}.version`)
  if (
    readFileIfPresent(versionMarker) === UV_VERSION &&
    destinations.every((path) => existsSync(path) && statSync(path).size > 0)
  ) {
    log(`uv ${UV_VERSION} sidecars already staged`)
    return
  }

  const release = resolveUvRelease(target)
  const work = mkdtempSync(join(tmpdir(), "iyw-claw-uv-"))
  try {
    const archive = join(work, `uv.${release.extension}`)
    const extracted = join(work, "extracted")
    mkdirSync(extracted, { recursive: true })
    log(`downloading uv ${UV_VERSION} from ${release.url}`)
    const response = await download(release.url, "uv")
    const bytes = Buffer.from(await response.arrayBuffer())
    const checksumResponse = await download(
      `${release.url}.sha256`,
      "uv checksum"
    )
    const expected = parseSha256(await checksumResponse.text())
    const actual = createHash("sha256").update(bytes).digest("hex")
    if (actual !== expected) die(`uv checksum mismatch: expected ${expected}, got ${actual}`)
    writeFileSync(archive, bytes)
    const extractor = resolveExtractor(archive, extracted, isWindows)
    execFileSync(extractor.command, extractor.args, {
      stdio: "inherit",
    })

    for (const name of ["uv", "uvx"]) {
      const source = findFile(extracted, `${name}${ext}`)
      if (!source) die(`${name}${ext} missing from uv archive`)
      const destination = join(BINARIES_DIR, `${name}-${target}${ext}`)
      copyFileIfChanged(source, destination)
      if (!isWindows) chmodSync(destination, 0o755)
      log(`staged ${name} sidecar at ${destination}`)
    }
    writeFileSync(versionMarker, UV_VERSION)
  } finally {
    rmSync(work, { recursive: true, force: true })
  }
}

function readFileIfPresent(path) {
  try {
    return readFileSync(path, "utf8").trim()
  } catch {
    return null
  }
}

async function main() {
  if (process.env.IYW_CLAW_SKIP_SIDECAR === "1") {
    log("IYW_CLAW_SKIP_SIDECAR=1 — skipping sidecar preparation")
    return
  }

  const { target: cliTarget, uvOnly } = parseArgs(process.argv.slice(2))
  const target =
    cliTarget || process.env.TAURI_TARGET_TRIPLE || resolveHostTriple()
  const isWindows = target.includes("windows")
  const ext = isWindows ? ".exe" : ""

  log(`target triple: ${target}`)
  if (uvOnly) {
    await stageUvSidecars(target, isWindows)
    return
  }

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

  await stageUvSidecars(target, isWindows)
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  await main()
}
