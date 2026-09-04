#!/usr/bin/env node

import { execFileSync } from "node:child_process"
import { createHash } from "node:crypto"
import {
  existsSync,
  mkdirSync,
  mkdtempSync,
  readFileSync,
  rmSync,
  statSync,
  writeFileSync,
} from "node:fs"
import { dirname, join, resolve } from "node:path"
import { fileURLToPath } from "node:url"
import { tmpdir } from "node:os"
import process from "node:process"

const SCRIPT_PATH = fileURLToPath(import.meta.url)
const SRC_TAURI = resolve(dirname(SCRIPT_PATH), "..")
const BINARIES_DIR = join(SRC_TAURI, "binaries")
const VERSION = "0.36.0"
const DOWNLOAD_TIMEOUT_MS = 5 * 60 * 1000
const DOWNLOAD_RETRIES = 5
const ASSETS = {
  "x86_64-pc-windows-msvc": [
    "agent-browser-win32-x64.exe",
    13837312,
    "412ff72737a109e93f5304b0ff76c988fb6f1f451d0fc7e010577922bcc20ff3",
  ],
  "x86_64-apple-darwin": [
    "agent-browser-darwin-x64",
    13510280,
    "45d9ac061a7d72e61eaff905326e2e19365f4dadb12142ea2f2d76d84689c708",
  ],
  "aarch64-apple-darwin": [
    "agent-browser-darwin-arm64",
    12363200,
    "b2106ab39db0838e7b1772f7f26f760518de56d09053150c56f9dddf15af997d",
  ],
  "x86_64-unknown-linux-gnu": [
    "agent-browser-linux-x64",
    14156776,
    "56d15181e51e00213f907fcf39707cfc76bfa804ff20f5a9373661c73f96de5e",
  ],
  "aarch64-unknown-linux-gnu": [
    "agent-browser-linux-arm64",
    12442720,
    "aeb556addca3903601a433de1acad3ace1c9c61d170084bf58d875884599a990",
  ],
}

function log(message) {
  console.log(`[prepare-agent-browser] ${message}`)
}

function parseTarget(argv) {
  for (let index = 0; index < argv.length; index += 1) {
    const arg = argv[index]
    if (arg === "--target" && argv[index + 1]) return argv[index + 1]
    if (arg.startsWith("--target=")) return arg.slice("--target=".length)
  }
  return null
}

function resolveHostTriple() {
  const output = execFileSync("rustc", ["-vV"], { encoding: "utf8" })
  const line = output.split(/\r?\n/).find((value) => value.startsWith("host:"))
  if (!line) throw new Error("rustc -vV did not report a host triple")
  return line.replace(/^host:\s*/, "").trim()
}

function sha256(bytes) {
  return createHash("sha256").update(bytes).digest("hex")
}

function verifyVendorBytes(bytes, expectedSize, expectedHash) {
  if (bytes.length !== expectedSize) {
    throw new Error(
      `download size mismatch: expected ${expectedSize}, received ${bytes.length}`
    )
  }
  const digest = sha256(bytes)
  if (digest !== expectedHash) {
    throw new Error(`download SHA-256 mismatch: received ${digest}`)
  }
}

function resolveDownloadProxy(env = process.env) {
  return (
    env.HTTPS_PROXY?.trim() ||
    env.HTTP_PROXY?.trim() ||
    env.ALL_PROXY?.trim() ||
    ""
  )
}

function curlExecutable() {
  return process.platform === "win32" ? "curl.exe" : "curl"
}

function downloadVendorBinary(asset, expectedSize, expectedHash) {
  log(`downloading agent-browser v${VERSION}`)
  const temporaryDirectory = mkdtempSync(join(tmpdir(), "iyw-agent-browser-"))
  const temporaryFile = join(temporaryDirectory, asset)
  const proxy = resolveDownloadProxy()
  const args = [
    "--fail",
    "--location",
    "--silent",
    "--show-error",
    "--retry",
    String(DOWNLOAD_RETRIES),
    "--retry-connrefused",
    "--retry-delay",
    "2",
    "--connect-timeout",
    "20",
    "--max-time",
    String(DOWNLOAD_TIMEOUT_MS / 1000),
  ]
  if (proxy) args.push("--proxy", proxy)
  args.push(
    "--output",
    temporaryFile,
    `https://github.com/vercel-labs/agent-browser/releases/download/v${VERSION}/${asset}`
  )

  try {
    execFileSync(curlExecutable(), args, {
      encoding: "utf8",
      stdio: ["ignore", "pipe", "pipe"],
      timeout: DOWNLOAD_TIMEOUT_MS,
      windowsHide: true,
    })
    const bytes = readFileSync(temporaryFile)
    verifyVendorBytes(bytes, expectedSize, expectedHash)
    return bytes
  } finally {
    rmSync(temporaryDirectory, { recursive: true, force: true })
  }
}

function hasVendorDigest(path, expectedSize, expectedHash) {
  if (!existsSync(path) || statSync(path).size !== expectedSize) return false
  return sha256(readFileSync(path)) === expectedHash
}

function verifyVersion(path) {
  if (process.platform !== "win32" || process.arch !== "x64") return
  const output = execFileSync(path, ["--version"], {
    encoding: "utf8",
    timeout: 10_000,
    windowsHide: true,
  }).trim()
  if (output !== `agent-browser ${VERSION}`) {
    throw new Error(`unexpected version output: ${JSON.stringify(output)}`)
  }
}

function assertStaged(path, expectedSize, expectedHash) {
  if (!existsSync(path)) {
    throw new Error(`agent-browser sidecar is missing or empty: ${path}`)
  }
  verifyVendorBytes(readFileSync(path), expectedSize, expectedHash)
  verifyVersion(path)
}

function unsupportedWindowsTarget(target, env) {
  if (!target.includes("windows")) return false
  if (env.IYW_CLAW_BROWSER_SIDECAR_EXCLUDED === "1") return true
  throw new Error(
    "agent-browser has no supported asset for this Windows target; " +
      "set IYW_CLAW_BROWSER_SIDECAR_EXCLUDED=1 only when the Tauri config " +
      "also excludes the sidecar"
  )
}

export async function prepareAgentBrowserSidecar(target, env = process.env) {
  const spec = ASSETS[target]
  if (!spec) {
    if (unsupportedWindowsTarget(target, env)) {
      log(`target ${target} explicitly excludes agent-browser`)
    } else {
      log(`target ${target} does not bundle agent-browser`)
    }
    return null
  }

  const [asset, expectedSize, expectedHash] = spec
  mkdirSync(BINARIES_DIR, { recursive: true })
  const extension = target.includes("windows") ? ".exe" : ""
  const destination = join(BINARIES_DIR, `agent-browser-${target}${extension}`)
  if (!hasVendorDigest(destination, expectedSize, expectedHash)) {
    writeFileSync(
      destination,
      downloadVendorBinary(asset, expectedSize, expectedHash)
    )
    log(`verified vendor binary staged at ${destination}`)
  } else {
    log(`verified vendor binary unchanged at ${destination}`)
  }

  verifyVersion(destination)
  assertStaged(destination, expectedSize, expectedHash)
  return destination
}

async function main() {
  const target =
    parseTarget(process.argv.slice(2)) ||
    process.env.TAURI_TARGET_TRIPLE ||
    resolveHostTriple()
  await prepareAgentBrowserSidecar(target)
}

if (process.argv[1] && resolve(process.argv[1]) === resolve(SCRIPT_PATH)) {
  try {
    await main()
  } catch (error) {
    console.error(`[prepare-agent-browser][ERROR] ${error.message}`)
    process.exit(1)
  }
}
