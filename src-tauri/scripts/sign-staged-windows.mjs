#!/usr/bin/env node

import { spawnSync } from "node:child_process"
import { existsSync } from "node:fs"
import { resolve } from "node:path"
import { setTimeout as delay } from "node:timers/promises"
import { fileURLToPath } from "node:url"
import process from "node:process"

import {
  buildSigntoolArgs,
  discoverSigntool,
  redactSigntoolArgs,
  resolveSignMode,
} from "./sign-windows.mjs"
import { verifyStagedSignature } from "./staged-signature-verification.mjs"

const ROOT = resolve(fileURLToPath(new URL("../..", import.meta.url)))
const SIGN_TIMEOUT_MS = 180_000
const TIMESTAMP_TIMEOUT_MS = 60_000
const TIMESTAMP_ATTEMPTS = 3
const RETRY_DELAY_MS = 5000

function splitCommands(args, file) {
  const timestampIndex = args.indexOf("/tr")
  const digestIndex = args.indexOf("/td")
  const omitted = new Set([
    timestampIndex,
    timestampIndex + 1,
    digestIndex,
    digestIndex + 1,
  ])
  return {
    sign: args.filter((_, index) => !omitted.has(index)),
    timestamp: [
      "timestamp",
      "/tr",
      args[timestampIndex + 1],
      "/td",
      args[digestIndex + 1],
      file,
    ],
  }
}

function invoke(signtool, args, env) {
  const phase = args[0]
  console.log(`[sign-staged-windows] ${redactSigntoolArgs(args).join(" ")}`)
  const result = spawnSync(signtool, args, {
    cwd: ROOT,
    env,
    stdio: "inherit",
    windowsHide: false,
    timeout: phase === "sign" ? SIGN_TIMEOUT_MS : TIMESTAMP_TIMEOUT_MS,
    killSignal: "SIGTERM",
  })
  if (result.error && result.error.code !== "ETIMEDOUT") throw result.error
  return result
}

async function timestampFile(context, args) {
  for (let attempt = 1; attempt <= TIMESTAMP_ATTEMPTS; attempt += 1) {
    const result = invoke(context.signtool, args, context.env)
    if (verifyStagedSignature(context)) return
    const reason = result.error?.code || `exit ${result.status}`
    if (attempt === TIMESTAMP_ATTEMPTS)
      throw new Error(
        `timestamp or signature verification failed (${reason}): ${context.file}`
      )
    console.warn(
      `[sign-staged-windows][WARN] timestamp attempt ${attempt}/${TIMESTAMP_ATTEMPTS} failed (${reason}); retrying without accessing the token`
    )
    await delay(RETRY_DELAY_MS * attempt)
  }
}

async function sign(file, env = process.env) {
  if (!existsSync(file)) throw new Error(`nothing to sign at ${file}`)
  const mode = resolveSignMode(env)
  if (mode === "none") throw new Error("staged signing requires a signing mode")
  const thumbprint = (env.IYW_CLAW_SIGN_THUMBPRINT || "").replace(/[\s:]/g, "")
  if (mode === "signtool" && !/^[a-f0-9]{40}$/i.test(thumbprint))
    throw new Error("staged signing requires a valid certificate thumbprint")
  const context = { signtool: discoverSigntool(env), file, env }
  const commands = splitCommands(buildSigntoolArgs(mode, file, env), file)
  if (mode === "signtool" && verifyStagedSignature(context)) {
    console.log(`[sign-staged-windows] already signed and timestamped: ${file}`)
    return
  }
  const result = invoke(context.signtool, commands.sign, env)
  if (mode !== "signtool" && (result.error || result.status !== 0))
    throw new Error(
      `signing failed (${result.error?.code || result.status}): ${file}`
    )
  if (!verifyStagedSignature({ ...context, timestamp: false })) {
    const reason = result.error?.code || `exit ${result.status}`
    throw new Error(
      `hardware signing or certificate verification failed (${reason}): ${file}`
    )
  }
  await timestampFile(context, commands.timestamp)
  console.log(`[sign-staged-windows] verified signer and timestamp: ${file}`)
}

const input = process.argv[2]
if (!input || process.argv.length !== 3) {
  console.error("Usage: sign-staged-windows.mjs <file>")
  process.exit(2)
}

try {
  await sign(resolve(input))
} catch (error) {
  console.error(`[sign-staged-windows][ERROR] ${error.message}`)
  process.exit(1)
}
