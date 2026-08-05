#!/usr/bin/env node

// Emit a Tauri config overlay that routes Windows bundling through
// `sign-windows.mjs` (Authenticode). The overlay is generated rather than
// committed because `signCommand` needs an absolute path: the bundler's working
// directory when it invokes the command is not part of Tauri's contract, so a
// relative path would be a silent coin flip.
//
// It is deliberately NOT named `tauri.windows.conf.json` — that filename is
// auto-merged by the Tauri CLI on every Windows build, which would force every
// local `pnpm tauri:build:*` to go looking for a certificate. Signing stays
// opt-in via `--sign` (build-desktop.mjs) or an explicit `--config` flag.
//
// Usage:
//   node src-tauri/scripts/prepare-signing-config.mjs           # writes overlay, prints path
//   node src-tauri/scripts/prepare-signing-config.mjs --print    # prints path only

import { writeFileSync } from "node:fs"
import { dirname, join, resolve } from "node:path"
import { fileURLToPath } from "node:url"
import process from "node:process"

import { resolveSignMode } from "./sign-windows.mjs"

const SCRIPT_PATH = fileURLToPath(import.meta.url)
const SRC_TAURI = resolve(dirname(SCRIPT_PATH), "..")

/** Generated overlay path. Gitignored — regenerated on every signed build. */
export const SIGNING_CONFIG_PATH = join(SRC_TAURI, ".signing.conf.json")

/**
 * Build the Tauri config overlay for a signed Windows build.
 *
 * `signCommand` wins over `certificateThumbprint`: it is the only form that
 * works for cloud HSM / USB-token CAs (post-2023 CA/B rules put OV keys on
 * hardware, so there is no importable .pfx to point a thumbprint at) and it is
 * the only form that can dispatch to Azure Artifact Signing.
 *
 * Nothing else belongs in this overlay. `digestAlgorithm`, `timestampUrl` and
 * `certificateThumbprint` only feed Tauri's *built-in* signtool invocation and
 * are ignored once `signCommand` is set — listing them here would create a
 * second, silently-dead source of truth for the timestamp server. Both live on
 * `IYW_CLAW_SIGN_*` env vars that `sign-windows.mjs` reads.
 *
 * `%1` is Tauri's placeholder for the file being signed; the bundler replaces
 * it before spawning. Passing it as its own argv element keeps paths with
 * spaces intact.
 */
export function buildSigningConfig() {
  return {
    bundle: {
      windows: {
        signCommand: {
          cmd: process.execPath,
          args: [join(SRC_TAURI, "scripts", "sign-windows.mjs"), "%1"],
        },
      },
    },
  }
}

function main() {
  const printOnly = process.argv.slice(2).includes("--print")
  const mode = resolveSignMode(process.env)

  if (mode === "none") {
    console.error(
      "[signing-config][ERROR] IYW_CLAW_SIGN_MODE is unset or 'none'.\n" +
        "  A signed build needs one of: signtool | pfx | azure.\n" +
        "  See docs/windows-code-signing.md for the variables each mode needs."
    )
    process.exit(1)
  }

  if (!printOnly) {
    const config = buildSigningConfig()
    // JSON.stringify escapes Windows backslashes, so the emitted path is a
    // valid JSON string literal without manual escaping.
    writeFileSync(
      SIGNING_CONFIG_PATH,
      `${JSON.stringify(config, null, 2)}\n`,
      "utf8"
    )
    console.error(
      `[signing-config] mode=${mode} overlay=${SIGNING_CONFIG_PATH}`
    )
  }

  // stdout carries only the path so CI can capture it:
  //   CONFIG=$(node .../prepare-signing-config.mjs)
  console.log(SIGNING_CONFIG_PATH)
}

if (process.argv[1] && resolve(process.argv[1]) === resolve(SCRIPT_PATH)) {
  try {
    main()
  } catch (error) {
    console.error(`[signing-config][ERROR] ${error.message}`)
    process.exit(1)
  }
}
