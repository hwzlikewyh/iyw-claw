#!/usr/bin/env node
//
// Prepare third-party sidecars before `tauri build` / `tauri dev`.
//
// What it does:
//   1. Resolves the target triple — `--target <triple>` arg, or
//      `TAURI_TARGET_TRIPLE` env, or the host's `rustc -vV` host triple.
// The main process exposes the built-in MCP server over Streamable HTTP. No
// first-party MCP executable is built or staged here.
//
// Skippable: set `IYW_CLAW_SKIP_SIDECAR=1` when iterating on the frontend
// and the optional browser sidecar is not needed.
//
// Intentionally Node-only (no shell): runs identically on macOS, Linux,
// Windows GitHub runners.

import { execFileSync } from "node:child_process"
import { existsSync, readdirSync, unlinkSync } from "node:fs"
import { dirname, join, resolve } from "node:path"
import { fileURLToPath } from "node:url"
import process from "node:process"

import { prepareAgentBrowserSidecar } from "./prepare-agent-browser-sidecar.mjs"

const SRC_TAURI = resolve(dirname(fileURLToPath(import.meta.url)), "..")

function log(msg) {
  console.log(`[prepare-sidecars] ${msg}`)
}

function removeLegacyMcpSidecars() {
  const binariesDir = resolve(SRC_TAURI, "binaries")
  if (!existsSync(binariesDir)) return
  const legacyPattern = /^iyw-claw-mcp(?:-|\.|$)/i
  const removed = readdirSync(binariesDir, { withFileTypes: true })
    .filter((entry) => entry.isFile() && legacyPattern.test(entry.name))
    .map((entry) => {
      unlinkSync(join(binariesDir, entry.name))
      return entry.name
    })
  if (removed.length > 0) {
    log(`removed ${removed.length} legacy MCP sidecar(s)`)
  }
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
    throw new Error(`cannot determine host triple via rustc -vV: ${e.message}`)
  }
}

async function main() {
  if (process.env.IYW_CLAW_SKIP_SIDECAR === "1") {
    log("IYW_CLAW_SKIP_SIDECAR=1 — skipping sidecar preparation")
    return
  }

  removeLegacyMcpSidecars()

  const { target: cliTarget } = parseArgs(process.argv.slice(2))
  const configuredTarget = cliTarget || process.env.TAURI_TARGET_TRIPLE

  const hostTarget = resolveHostTriple()
  const target = configuredTarget || hostTarget
  log(`target triple: ${target}`)
  await prepareAgentBrowserSidecar(target)
}

if (
  process.argv[1] &&
  resolve(process.argv[1]) === fileURLToPath(import.meta.url)
) {
  await main()
}
