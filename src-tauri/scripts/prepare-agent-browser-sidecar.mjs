#!/usr/bin/env node

import { execFileSync } from "node:child_process"
import { createHash } from "node:crypto"
import {
  existsSync,
  mkdirSync,
  readFileSync,
  statSync,
  writeFileSync,
} from "node:fs"
import { dirname, join, resolve } from "node:path"
import { fileURLToPath } from "node:url"
import process from "node:process"

const SCRIPT_PATH = fileURLToPath(import.meta.url)
const SRC_TAURI = resolve(dirname(SCRIPT_PATH), "..")
const BINARIES_DIR = join(SRC_TAURI, "binaries")
const VERSION = "0.35.2"
const ASSETS = {
  "x86_64-pc-windows-msvc": ["agent-browser-win32-x64.exe", 13707264, "5ffcad90cda06114730e8b202285c45ec0866d1b8d7876b561329e4a8cfbb126"],
  "x86_64-apple-darwin": ["agent-browser-darwin-x64", 13378880, "d76cfc76885d5007f3c119008a80a145b381ec4dfdd202f43e46cd0829751774"],
  "aarch64-apple-darwin": ["agent-browser-darwin-arm64", 12247424, "e1e08f3b0a1c711750209e6a25b6f3a9dab7ed6e6a24b55a2556050b991fcc97"],
  "x86_64-unknown-linux-gnu": ["agent-browser-linux-x64", 14021032, "b699f24eebdb7fde91a34a9d697a1b84c3145f54327b60694b46f06b2972ce4d"],
  "aarch64-unknown-linux-gnu": ["agent-browser-linux-arm64", 12332896, "1599fec4f4e75dc26fc08eecc06ca4b729a0361932b32a6afb99885f0f829ecb"],
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

async function downloadVendorBinary(asset, expectedSize, expectedHash) {
  log(`downloading agent-browser v${VERSION}`)
  const response = await fetch(
    `https://github.com/vercel-labs/agent-browser/releases/download/v${VERSION}/${asset}`,
    { redirect: "follow", signal: AbortSignal.timeout(60_000) }
  )
  if (!response.ok) {
    throw new Error(`download failed with HTTP ${response.status}`)
  }
  const bytes = await readBoundedResponse(response, expectedSize)
  verifyVendorBytes(bytes, expectedSize, expectedHash)
  return bytes
}

async function readBoundedResponse(response, expectedSize) {
  const reader = response.body?.getReader()
  if (!reader) throw new Error("download response body is unavailable")
  const chunks = []
  let total = 0
  while (true) {
    const { done, value } = await reader.read()
    if (done) return Buffer.concat(chunks, total)
    total += value.byteLength
    if (total > expectedSize) {
      await reader.cancel()
      throw new Error(`download exceeds expected size: received over ${total}`)
    }
    chunks.push(Buffer.from(value))
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
      await downloadVendorBinary(asset, expectedSize, expectedHash)
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
