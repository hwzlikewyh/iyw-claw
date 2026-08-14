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
const SUPPORTED_TARGET = "x86_64-pc-windows-msvc"
const VERSION = "0.34.0"
const EXPECTED_SIZE = 13_580_288
const EXPECTED_SHA256 =
  "604820a9e86cdb8bba46da737fc0edb31bc92de6691c73dbc61d3673c370a6b5"
const DOWNLOAD_URL =
  `https://github.com/vercel-labs/agent-browser/releases/download/v${VERSION}/` +
  "agent-browser-win32-x64.exe"

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

function verifyVendorBytes(bytes) {
  if (bytes.length !== EXPECTED_SIZE) {
    throw new Error(
      `download size mismatch: expected ${EXPECTED_SIZE}, received ${bytes.length}`
    )
  }
  const digest = sha256(bytes)
  if (digest !== EXPECTED_SHA256) {
    throw new Error(`download SHA-256 mismatch: received ${digest}`)
  }
}

async function downloadVendorBinary() {
  log(`downloading agent-browser v${VERSION}`)
  const response = await fetch(DOWNLOAD_URL, {
    redirect: "follow",
    signal: AbortSignal.timeout(60_000),
  })
  if (!response.ok) {
    throw new Error(`download failed with HTTP ${response.status}`)
  }
  const bytes = await readBoundedResponse(response)
  verifyVendorBytes(bytes)
  return bytes
}

async function readBoundedResponse(response) {
  const reader = response.body?.getReader()
  if (!reader) throw new Error("download response body is unavailable")
  const chunks = []
  let total = 0
  while (true) {
    const { done, value } = await reader.read()
    if (done) return Buffer.concat(chunks, total)
    total += value.byteLength
    if (total > EXPECTED_SIZE) {
      await reader.cancel()
      throw new Error(`download exceeds expected size: received over ${total}`)
    }
    chunks.push(Buffer.from(value))
  }
}

function hasVendorDigest(path) {
  if (!existsSync(path) || statSync(path).size !== EXPECTED_SIZE) return false
  return sha256(readFileSync(path)) === EXPECTED_SHA256
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

function assertStaged(path) {
  if (!existsSync(path)) {
    throw new Error(`agent-browser sidecar is missing or empty: ${path}`)
  }
  verifyVendorBytes(readFileSync(path))
  verifyVersion(path)
}

function unsupportedWindowsTarget(target, env) {
  if (!target.includes("windows")) return false
  if (env.IYW_CLAW_BROWSER_SIDECAR_EXCLUDED === "1") return true
  throw new Error(
    `agent-browser supports only ${SUPPORTED_TARGET}; ` +
      "set IYW_CLAW_BROWSER_SIDECAR_EXCLUDED=1 only when the Tauri config " +
      "also excludes the sidecar"
  )
}

export async function prepareAgentBrowserSidecar(target, env = process.env) {
  if (target !== SUPPORTED_TARGET) {
    if (unsupportedWindowsTarget(target, env)) {
      log(`target ${target} explicitly excludes agent-browser`)
    } else {
      log(`target ${target} does not bundle agent-browser`)
    }
    return null
  }

  mkdirSync(BINARIES_DIR, { recursive: true })
  const destination = join(BINARIES_DIR, `agent-browser-${target}.exe`)
  if (!hasVendorDigest(destination)) {
    writeFileSync(destination, await downloadVendorBinary())
    log(`verified vendor binary staged at ${destination}`)
  } else {
    log(`verified vendor binary unchanged at ${destination}`)
  }

  verifyVersion(destination)
  assertStaged(destination)
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
