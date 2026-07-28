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
  renameSync,
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
const APP_VERSION = JSON.parse(
  readFileSync(resolve(SRC_TAURI, "..", "package.json"), "utf8")
).version
const UV_VERSION = "0.8.10"
const DOWNLOAD_TIMEOUT_MS = 5 * 60 * 1000
const DOWNLOAD_ATTEMPTS = 3
const DOWNLOAD_RETRY_DELAY_MS = 2 * 1000

// ─── Bundled runtime resources ───────────────────────────────────────────
// Node.js and MinGit archives are pre-staged into resources/runtime/downloads/
// so the first-launch runtime_bootstrap can use them without a network round-trip.
// @agentclientprotocol/codex-acp is pre-installed into a private npm prefix and
// zipped into resources/runtime/npm/ so acpPrepareNpxAgent never hits the network.

const CODEX_ACP_VERSION = "1.1.5"
const CODEX_ACP_PACKAGE = `@agentclientprotocol/codex-acp@${CODEX_ACP_VERSION}`
const CODEX_ACP_REGISTRY =
  process.env.IYW_CLAW_NPM_REGISTRY || "https://registry.npmmirror.com"
const RESOURCES_DIR = join(SRC_TAURI, "resources", "runtime")
const RESOURCES_DOWNLOADS_DIR = join(RESOURCES_DIR, "downloads")
const RESOURCES_NPM_DIR = join(RESOURCES_DIR, "npm")

// Pinned specs per Tauri target triple — mirrors runtime_bootstrap.rs
// (NODE_VERSION_X64, NODE_VERSION_X86, GIT_VERSION, sha256 values).  These
// MUST stay in sync with the Rust constants; a mismatch is silently safe (Rust
// re-verifies against its own pinned hash and falls back to the network) but
// would waste CI time staging an archive that will never pass.
const NODE_GIT_SPECS = {
  "x86_64-pc-windows-msvc": {
    node: {
      version: "24.0.0",
      asset: "node-v24.0.0-win-x64.zip",
      mirror:
        "https://registry.npmmirror.com/-/binary/node/v24.0.0/node-v24.0.0-win-x64.zip",
      official: "https://nodejs.org/dist/v24.0.0/node-v24.0.0-win-x64.zip",
      sha256:
        "3d0fff80c87bb9a8d7f49f2f27832aa34a1477d137af46f5b14df5498be81304",
    },
    git: {
      asset: "MinGit-2.55.0.2-64-bit.zip",
      mirror:
        "https://registry.npmmirror.com/-/binary/git-for-windows/v2.55.0.windows.2/MinGit-2.55.0.2-64-bit.zip",
      official:
        "https://github.com/git-for-windows/git/releases/download/v2.55.0.windows.2/MinGit-2.55.0.2-64-bit.zip",
      sha256:
        "e3ea2944cea4b3fabcd69c7c1669ef69b1b66c05ac7806d81224d0abad2dec31",
    },
    codex: { npmOs: "win32", npmCpu: "x64" },
  },
  "aarch64-pc-windows-msvc": {
    node: {
      version: "24.0.0",
      asset: "node-v24.0.0-win-arm64.zip",
      mirror:
        "https://registry.npmmirror.com/-/binary/node/v24.0.0/node-v24.0.0-win-arm64.zip",
      official: "https://nodejs.org/dist/v24.0.0/node-v24.0.0-win-arm64.zip",
      sha256:
        "03b6676f4872fbe4645113de8e23da834a7c1464045369f2b7a374bf482a5e12",
    },
    git: {
      asset: "MinGit-2.55.0.2-arm64.zip",
      mirror:
        "https://registry.npmmirror.com/-/binary/git-for-windows/v2.55.0.windows.2/MinGit-2.55.0.2-arm64.zip",
      official:
        "https://github.com/git-for-windows/git/releases/download/v2.55.0.windows.2/MinGit-2.55.0.2-arm64.zip",
      sha256:
        "0b2b81fdce284efd174cbb51b886ccea2fd271679c4b5c21f07d9e03bae51413",
    },
    codex: { npmOs: "win32", npmCpu: "arm64" },
  },
  "i686-pc-windows-msvc": {
    node: {
      version: "22.23.1",
      asset: "node-v22.23.1-win-x86.zip",
      mirror:
        "https://registry.npmmirror.com/-/binary/node/v22.23.1/node-v22.23.1-win-x86.zip",
      official: "https://nodejs.org/dist/v22.23.1/node-v22.23.1-win-x86.zip",
      sha256:
        "e298b368aad86c571447a3650db3ce19063373ffd39d6d73d014a5d9ad31dc62",
    },
    git: {
      asset: "MinGit-2.55.0.2-32-bit.zip",
      mirror:
        "https://registry.npmmirror.com/-/binary/git-for-windows/v2.55.0.windows.2/MinGit-2.55.0.2-32-bit.zip",
      official:
        "https://github.com/git-for-windows/git/releases/download/v2.55.0.windows.2/MinGit-2.55.0.2-32-bit.zip",
      sha256:
        "04009f6150c1cec2d6779c51406c8c6a3f0133e57fa91c91eb8a030b93e68ccb",
    },
    // No @openai/codex win32-ia32 optional dep; skip codex bundling for 32-bit.
    codex: null,
  },
}

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
    command: isWindows ? win32.join(windowsRoot, "System32", "tar.exe") : "tar",
    args: ["-xf", archive, "-C", destination],
  }
}

async function download(url, label, readBody) {
  for (let attempt = 1; attempt <= DOWNLOAD_ATTEMPTS; attempt++) {
    try {
      const response = await fetch(url, {
        signal: AbortSignal.timeout(DOWNLOAD_TIMEOUT_MS),
      })
      if (!response.ok) throw new Error(`HTTP ${response.status}`)
      return await readBody(response)
    } catch (error) {
      const reason = error instanceof Error ? error.message : String(error)
      if (attempt === DOWNLOAD_ATTEMPTS) {
        die(
          `${label} download failed after ${DOWNLOAD_ATTEMPTS} attempts: ${reason}`
        )
      }
      const delay = DOWNLOAD_RETRY_DELAY_MS * attempt
      log(
        `${label} download attempt ${attempt}/${DOWNLOAD_ATTEMPTS} failed: ${reason}; retrying in ${delay}ms`
      )
      await new Promise((resolve) => setTimeout(resolve, delay))
    }
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
    const bytes = Buffer.from(
      await download(release.url, "uv", (response) => response.arrayBuffer())
    )
    const expected = parseSha256(
      await download(`${release.url}.sha256`, "uv checksum", (response) =>
        response.text()
      )
    )
    const actual = createHash("sha256").update(bytes).digest("hex")
    if (actual !== expected)
      die(`uv checksum mismatch: expected ${expected}, got ${actual}`)
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

async function stageNodeGitArchives(target) {
  const spec = NODE_GIT_SPECS[target]
  if (!spec) {
    log(`no node/git archives for target ${target} — skipping`)
    return
  }

  mkdirSync(RESOURCES_DOWNLOADS_DIR, { recursive: true })

  for (const [component, { asset, mirror, official, sha256 }] of Object.entries({
    node: spec.node,
    git: spec.git,
  })) {
    const dest = join(RESOURCES_DOWNLOADS_DIR, asset)
    if (existsSync(dest)) {
      const actual = createHash("sha256")
        .update(readFileSync(dest))
        .digest("hex")
      if (actual === sha256) {
        log(`${component} archive already staged: ${asset}`)
        continue
      }
      log(`${component} archive checksum mismatch, re-downloading`)
    }

    log(`staging ${component} archive: ${asset}`)
    let lastError = null
    for (const [source, url] of [
      ["mirror", mirror],
      ["official", official],
    ]) {
      try {
        const bytes = Buffer.from(
          await download(url, `${component} ${source}`, (r) => r.arrayBuffer())
        )
        const actual = createHash("sha256").update(bytes).digest("hex")
        if (actual !== sha256) {
          throw new Error(`checksum mismatch: expected ${sha256}, got ${actual}`)
        }
        writeFileSync(dest, bytes)
        log(`staged ${component} archive from ${source}: ${asset}`)
        lastError = null
        break
      } catch (error) {
        const msg = error instanceof Error ? error.message : String(error)
        log(`${component} ${source} failed: ${msg}`)
        lastError = `${source}: ${msg}`
      }
    }
    if (lastError) {
      die(`all sources failed for ${component} ${asset} — ${lastError}`)
    }
  }
}

/** Map from Tauri target triple to the same platform token that
 *  `registry::current_platform()` returns on the running binary.
 *  Used to name the bundle so the Rust seeder can find it with a trivial
 *  `format!("codex-acp-{version}-{current_platform()}.zip")` call.
 */
function registryPlatformFor(target) {
  const map = {
    "x86_64-pc-windows-msvc": "windows-x86_64",
    "aarch64-pc-windows-msvc": "windows-aarch64",
    "i686-pc-windows-msvc": "windows-i686",
    "x86_64-apple-darwin": "darwin-x86_64",
    "aarch64-apple-darwin": "darwin-aarch64",
    "x86_64-unknown-linux-gnu": "linux-x86_64",
    "aarch64-unknown-linux-gnu": "linux-aarch64",
  }
  return map[target] ?? null
}

async function stageCodexBundle(target, isWindows) {
  const spec = NODE_GIT_SPECS[target]
  if (!spec?.codex) {
    log(`no codex bundle spec for target ${target} — skipping`)
    return
  }

  const registryPlatform = registryPlatformFor(target)
  if (!registryPlatform) {
    log(`unknown registry platform for ${target} — skipping codex bundle`)
    return
  }

  mkdirSync(RESOURCES_NPM_DIR, { recursive: true })

  const bundleAsset = `codex-acp-${CODEX_ACP_VERSION}-${registryPlatform}.zip`
  const bundleDest = join(RESOURCES_NPM_DIR, bundleAsset)
  if (existsSync(bundleDest)) {
    log(`codex bundle already staged: ${bundleAsset}`)
    return
  }

  log(`building codex npm prefix for ${target}...`)
  const work = mkdtempSync(join(tmpdir(), "iyw-claw-codex-"))
  try {
    const prefixDir = join(work, "prefix")
    mkdirSync(prefixDir, { recursive: true })

    const npmArgs = [
      "install",
      "--global",
      "--include=optional",
      `--registry=${CODEX_ACP_REGISTRY}`,
      `--prefix=${prefixDir}`,
      `--os=${spec.codex.npmOs}`,
      `--cpu=${spec.codex.npmCpu}`,
      CODEX_ACP_PACKAGE,
    ]
    log(`$ npm ${npmArgs.join(" ")}`)
    execFileSync("npm", npmArgs, {
      stdio: "inherit",
      cwd: work,
    })

    // Verify the command shim exists
    const cmdName = isWindows ? "codex-acp.cmd" : "codex-acp"
    const cmdPath = join(prefixDir, cmdName)
    if (!existsSync(cmdPath)) {
      die(`npm install succeeded but ${cmdName} is missing from prefix`)
    }

    // Zip the prefix — use Python zipfile for cross-platform consistency
    log(`zipping ${prefixDir} -> ${bundleAsset}`)
    const zipScript = `
import zipfile, os, sys
src, out = sys.argv[1], sys.argv[2]
with zipfile.ZipFile(out, 'w', zipfile.ZIP_DEFLATED) as zf:
    for root, dirs, files in os.walk(src):
        for fn in files:
            fp = os.path.join(root, fn)
            zf.write(fp, os.path.relpath(fp, src))
size_mb = os.path.getsize(out) / 1024 / 1024
print(f"Created {os.path.basename(out)} ({size_mb:.1f} MB)")
`.trim()
    const pythonCmd = isWindows ? "python" : "python3"
    execFileSync(pythonCmd, ["-c", zipScript, prefixDir, bundleDest], {
      stdio: "inherit",
    })
    log(`codex bundle staged: ${bundleAsset}`)
  } finally {
    rmSync(work, { recursive: true, force: true })
  }
}

async function main() {
  if (process.env.IYW_CLAW_SKIP_SIDECAR === "1") {
    log("IYW_CLAW_SKIP_SIDECAR=1 — skipping sidecar preparation")
    return
  }

  const { target: cliTarget, uvOnly } = parseArgs(process.argv.slice(2))
  const configuredTarget = cliTarget || process.env.TAURI_TARGET_TRIPLE
  if (uvOnly) {
    const target = configuredTarget || resolveHostTriple()
    log(`target triple: ${target}`)
    await stageUvSidecars(target, target.includes("windows"))
    return
  }

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

  await stageUvSidecars(target, isWindows)
  // Pre-stage node/git archives and codex-acp npm bundle so the installer is
  // fully self-contained.  Non-fatal: a download failure is logged but does not
  // break the build — the app will fall back to its runtime download flow.
  try {
    await stageNodeGitArchives(target)
  } catch (error) {
    const msg = error instanceof Error ? error.message : String(error)
    log(`[WARN] node/git archive staging failed (installer will download at runtime): ${msg}`)
  }
  try {
    await stageCodexBundle(target, isWindows)
  } catch (error) {
    const msg = error instanceof Error ? error.message : String(error)
    log(`[WARN] codex bundle staging failed (installer will install via npm): ${msg}`)
  }
}

if (
  process.argv[1] &&
  resolve(process.argv[1]) === fileURLToPath(import.meta.url)
) {
  await main()
}
