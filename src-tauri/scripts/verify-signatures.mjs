#!/usr/bin/env node

/**
 * Verify Authenticode signatures on first-party Windows release executables.
 *
 * A signed build can still emit unsigned files: `signCommand` only covers what
 * the bundler hands it. Publishing one unsigned first-party .exe re-opens the
 * antivirus problem the certificate was bought to close, so this check is
 * meant to run between building and uploading.
 *
 * Usage:
 *   node src-tauri/scripts/verify-signatures.mjs            # verify, report, exit 1 on any unsigned
 *   node src-tauri/scripts/verify-signatures.mjs --warn     # report only, always exit 0
 *   node src-tauri/scripts/verify-signatures.mjs a.exe b.exe  # verify just these
 */

import { spawnSync } from "node:child_process"
import { existsSync, readdirSync, statSync } from "node:fs"
import { dirname, join, resolve } from "node:path"
import { fileURLToPath } from "node:url"
import process from "node:process"

import { discoverSigntool } from "./sign-windows.mjs"

const SCRIPT_PATH = fileURLToPath(import.meta.url)
const SRC_TAURI = resolve(dirname(SCRIPT_PATH), "..")
const REPO_ROOT = resolve(SRC_TAURI, "..")

/**
 * Cargo release output directories to search.
 *
 * A plain `tauri build` writes to `target/release`, but a `--target <triple>`
 * build (what CI does) writes to `target/<triple>/release`. Checking only the
 * first would make this script find zero artifacts on CI and "pass" a release
 * nobody verified, so scan both.
 */
export function releaseDirs(srcTauri = SRC_TAURI) {
  const target = join(srcTauri, "target")
  if (!existsSync(target)) return []

  const dirs = []
  const host = join(target, "release")
  if (existsSync(host)) dirs.push(host)

  for (const entry of readdirSync(target)) {
    if (entry === "release" || entry === "debug") continue
    const candidate = join(target, entry, "release")
    if (existsSync(candidate) && statSync(candidate).isDirectory()) {
      dirs.push(candidate)
    }
  }
  return dirs
}

/** Collect the release artifacts that must carry a signature. */
export function collectArtifacts(srcTauri = SRC_TAURI) {
  const artifacts = []

  for (const releaseDir of releaseDirs(srcTauri)) {
    // Main application binary.
    const mainExe = join(releaseDir, "iyw-claw.exe")
    if (existsSync(mainExe)) artifacts.push(mainExe)

    // NSIS installers, including the branded copies.
    const nsis = join(releaseDir, "bundle", "nsis")
    if (existsSync(nsis)) {
      for (const name of readdirSync(nsis)) {
        if (name.toLowerCase().endsWith("-setup.exe")) {
          artifacts.push(join(nsis, name))
        }
      }
    }
  }

  return [...new Set(artifacts)].sort()
}

/**
 * `/pa` selects the Authenticode policy — without it signtool uses the Windows
 * driver policy and reports valid app signatures as failures. `/all` checks
 * every signature present rather than only the first.
 */
export function verifyArgs(file) {
  return ["verify", "/pa", "/all", file]
}

/**
 * Classify a failed verification. Both outcomes fail the gate, but they need
 * different fixes, and calling an untrusted chain "unsigned" sends people
 * looking for the wrong bug:
 *
 *   unsigned  — no signature at all: signing never ran for this artifact.
 *   untrusted — a signature is present but its chain does not terminate in a
 *               trusted root: a self-signed certificate, or a real CA whose
 *               intermediate/root is missing on the verifying machine.
 */
export function classifyVerifyOutput(status, output) {
  if (status === 0) return "signed"
  return /not\s+trusted|terminated in a root certificate/i.test(output)
    ? "untrusted"
    : "unsigned"
}

function verifyOne(signtool, file) {
  const result = spawnSync(signtool, verifyArgs(file), {
    cwd: REPO_ROOT,
    encoding: "utf8",
  })
  if (result.error) throw result.error
  const output = `${result.stdout ?? ""}${result.stderr ?? ""}`.trim()
  const verdict = classifyVerifyOutput(result.status, output)
  return { file, verdict, signed: verdict === "signed", output }
}

export function parseVerifyOptions(argv) {
  const options = { warnOnly: false, targets: [] }
  for (const arg of argv) {
    if (arg === "--warn") {
      options.warnOnly = true
    } else if (arg.startsWith("--")) {
      throw new Error(`unknown option: ${arg}`)
    } else {
      // Explicit paths override discovery — useful for checking a single
      // artifact, or one that lives outside the release output tree.
      options.targets.push(arg)
    }
  }
  return options
}

export function main(argv = process.argv.slice(2)) {
  const { warnOnly, targets } = parseVerifyOptions(argv)

  if (process.platform !== "win32") {
    throw new Error(
      `signature verification needs Windows (signtool.exe); current platform is ${process.platform}`
    )
  }

  const artifacts = targets.length > 0 ? targets : collectArtifacts()
  if (artifacts.length === 0) {
    throw new Error(
      "no artifacts found to verify — build first (pnpm tauri:build:signed)"
    )
  }
  for (const file of artifacts) {
    if (!existsSync(file)) throw new Error(`nothing to verify at ${file}`)
  }

  const signtool = discoverSigntool()
  const results = artifacts.map((file) => verifyOne(signtool, file))
  const unsigned = results.filter((result) => !result.signed)

  const LABELS = {
    signed: "signed   ",
    unsigned: "UNSIGNED ",
    untrusted: "UNTRUSTED",
  }
  for (const result of results) {
    const relative = result.file.startsWith(REPO_ROOT)
      ? result.file.slice(REPO_ROOT.length + 1)
      : result.file
    console.log(`${LABELS[result.verdict]}  ${relative}`)
  }

  console.log(
    `\n[verify-signatures] ${results.length - unsigned.length}/${results.length} artifacts signed`
  )

  if (unsigned.length > 0) {
    console.error("\n[verify-signatures] problems:")
    for (const result of unsigned) {
      console.error(`  [${result.verdict}] ${result.file}`)
      if (result.output) {
        console.error(`    ${result.output.split("\n").join("\n    ")}`)
      }
    }
    if (results.some((result) => result.verdict === "untrusted")) {
      console.error(
        "\n  'untrusted' means a signature IS present but its chain has no trusted\n" +
          "  root here. Expected with a self-signed test certificate; with a real CA\n" +
          "  certificate it usually means the intermediate is missing from the bundle."
      )
    }
    if (!warnOnly) {
      throw new Error(
        `${unsigned.length} artifact(s) failed signature verification — do not publish this build`
      )
    }
  }
}

if (process.argv[1] && resolve(process.argv[1]) === resolve(SCRIPT_PATH)) {
  try {
    main()
  } catch (error) {
    console.error(`[verify-signatures][ERROR] ${error.message}`)
    process.exit(1)
  }
}
