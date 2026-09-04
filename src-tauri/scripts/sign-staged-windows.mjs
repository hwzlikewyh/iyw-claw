#!/usr/bin/env node

/**
 * Bounded Authenticode signer used only by the staged Windows workflow.
 * The regular release signer remains untouched.
 */

import { spawnSync } from "node:child_process"
import { existsSync } from "node:fs"
import { resolve } from "node:path"
import { fileURLToPath } from "node:url"
import process from "node:process"

import {
  buildSigntoolArgs,
  discoverSigntool,
  redactSigntoolArgs,
  resolveSignMode,
} from "./sign-windows.mjs"

const ROOT = resolve(fileURLToPath(new URL("../..", import.meta.url)))
// SafeNet token signing can exceed 90 seconds for NSIS temporary files.
// Keep the wait bounded so a missing login cannot block on PIN UI forever.
const TIMEOUT_MS = 180_000
const VERIFY_TIMEOUT_MS = 30_000

function verifySigned(signtool, file) {
  const result = spawnSync(signtool, ["verify", "/pa", "/all", file], {
    cwd: ROOT,
    stdio: "ignore",
    windowsHide: false,
    timeout: VERIFY_TIMEOUT_MS,
    killSignal: "SIGTERM",
  })
  if (result.error?.code === "ETIMEDOUT") {
    console.warn(
      `[sign-staged-windows][WARN] signature verification timed out: ${file}`
    )
    return false
  }
  return result.status === 0
}

function sign(file, env = process.env) {
  if (!existsSync(file)) throw new Error(`nothing to sign at ${file}`)
  const mode = resolveSignMode(env)
  if (mode === "none") throw new Error("staged signing requires a signing mode")
  const signtool = discoverSigntool(env)
  const args = buildSigntoolArgs(mode, file, env)
  const startedAt = Date.now()
  console.log(
    `[sign-staged-windows] signtool ${redactSigntoolArgs(args).join(" ")}`
  )
  const result = spawnSync(signtool, args, {
    cwd: ROOT,
    stdio: "inherit",
    windowsHide: false,
    timeout: TIMEOUT_MS,
    killSignal: "SIGTERM",
  })
  if (result.error?.code === "ETIMEDOUT") {
    if (verifySigned(signtool, file)) {
      console.warn(
        `[sign-staged-windows][WARN] timeout after completed signature: ${file} ` +
          `(elapsedMs=${Date.now() - startedAt})`
      )
      return
    }
    throw new Error(`signtool timed out before signing ${file}`)
  }
  if (result.error) throw result.error
  if (result.status !== 0) {
    throw new Error(`signtool exited with code ${result.status} for ${file}`)
  }
  console.log(
    `[sign-staged-windows] completed ${file} (elapsedMs=${Date.now() - startedAt})`
  )
}

const file = process.argv[2]
if (!file || process.argv.length !== 3) {
  console.error("Usage: sign-staged-windows.mjs <file>")
  process.exit(2)
}

try {
  sign(file)
} catch (error) {
  console.error(`[sign-staged-windows][ERROR] ${error.message}`)
  process.exit(1)
}
