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

function verifySigned(signtool, file, env) {
  const thumbprint = (env.IYW_CLAW_SIGN_THUMBPRINT || "")
    .replace(/\s/g, "")
    .toUpperCase()
  if (!/^[A-F0-9]{40}$/.test(thumbprint)) return false
  const result = spawnSync(signtool, ["verify", "/pa", "/all", file], {
    cwd: ROOT,
    stdio: "ignore",
    windowsHide: false,
  })
  if (result.status !== 0) return false
  // 探针源文件本身可能带厂商签名，必须确认已换成预期证书且有时间戳。
  const script = [
    "$ErrorActionPreference = 'Stop'",
    "$signature = Get-AuthenticodeSignature -LiteralPath $env:IYW_SIGN_VERIFY_PATH",
    "if ($signature.Status -ne 'Valid' -or $signature.SignerCertificate.Thumbprint -ne $env:IYW_SIGN_VERIFY_THUMBPRINT -or $null -eq $signature.TimeStamperCertificate) { exit 1 }",
  ].join("; ")
  return (
    spawnSync(
      "powershell.exe",
      ["-NoProfile", "-NonInteractive", "-Command", script],
      {
        env: {
          ...Object.fromEntries(
            Object.entries(env).filter(
              ([name]) => name.toUpperCase() !== "PSMODULEPATH"
            )
          ),
          IYW_SIGN_VERIFY_PATH: file,
          IYW_SIGN_VERIFY_THUMBPRINT: thumbprint,
        },
        timeout: 30_000,
        stdio: "ignore",
        windowsHide: true,
      }
    ).status === 0
  )
}

function sign(file, env = process.env) {
  if (!existsSync(file)) throw new Error(`nothing to sign at ${file}`)
  const mode = resolveSignMode(env)
  if (mode === "none") throw new Error("staged signing requires a signing mode")
  const signtool = discoverSigntool(env)
  const args = buildSigntoolArgs(mode, file, env)
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
    if (verifySigned(signtool, file, env)) {
      console.warn(
        `[sign-staged-windows][WARN] timeout after completed signature: ${file}`
      )
      return
    }
    throw new Error(`signtool timed out before signing ${file}`)
  }
  if (result.error) throw result.error
  if (result.status !== 0) {
    throw new Error(`signtool exited with code ${result.status} for ${file}`)
  }
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
