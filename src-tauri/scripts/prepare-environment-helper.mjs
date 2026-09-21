#!/usr/bin/env node

import { execFileSync } from "node:child_process"
import {
  copyFileSync,
  existsSync,
  mkdirSync,
  readFileSync,
  readdirSync,
  rmSync,
} from "node:fs"
import { dirname, join, resolve } from "node:path"
import { fileURLToPath } from "node:url"
import process from "node:process"

const SRC_TAURI = resolve(dirname(fileURLToPath(import.meta.url)), "..")
const MANIFEST = join(SRC_TAURI, "environment-core", "Cargo.toml")
const TARGET_DIR = join(SRC_TAURI, "target", "environment-helper")
const BINARIES = join(SRC_TAURI, "binaries")

function hostTarget() {
  const output = execFileSync("rustc", ["-vV"], { encoding: "utf8" })
  const line = output.split(/\r?\n/).find((value) => value.startsWith("host:"))
  if (!line) throw new Error("rustc did not report a host target")
  return line.slice("host:".length).trim()
}

function executableName(target) {
  return target.includes("windows") ? "iyw-environment.exe" : "iyw-environment"
}

function buildEnvironment(target) {
  const env = {
    ...process.env,
    IYW_ENVIRONMENT_PC_VERSION: JSON.parse(
      readFileSync(join(SRC_TAURI, "../package.json"), "utf8")
    ).version,
  }
  if (target.includes("windows")) {
    const flags =
      env.CARGO_ENCODED_RUSTFLAGS ||
      (env.RUSTFLAGS || "").trim().split(/\s+/).filter(Boolean).join("\u001f")
    env.CARGO_ENCODED_RUSTFLAGS = [flags, "-C", "target-feature=+crt-static"]
      .filter(Boolean)
      .join("\u001f")
  }
  return env
}

export function prepareEnvironmentHelper(target = null) {
  const resolvedTarget =
    target || process.env.TAURI_TARGET_TRIPLE || hostTarget()
  execFileSync(
    "cargo",
    [
      "build",
      "--release",
      "--locked",
      "--manifest-path",
      MANIFEST,
      "--target-dir",
      TARGET_DIR,
      "--target",
      resolvedTarget,
    ],
    {
      cwd: SRC_TAURI,
      stdio: "inherit",
      env: buildEnvironment(resolvedTarget),
    }
  )
  const source = join(
    TARGET_DIR,
    resolvedTarget,
    "release",
    executableName(resolvedTarget)
  )
  if (!existsSync(source))
    throw new Error(`environment helper missing: ${source}`)
  mkdirSync(BINARIES, { recursive: true })
  for (const entry of readdirSync(BINARIES)) {
    if (/^iyw-environment-/.test(entry)) rmSync(join(BINARIES, entry))
  }
  const suffix = resolvedTarget.includes("windows") ? ".exe" : ""
  const destination = join(
    BINARIES,
    `iyw-environment-${resolvedTarget}${suffix}`
  )
  copyFileSync(source, destination)
  console.log(`[prepare-environment-helper] staged ${destination}`)
  return destination
}

if (
  process.argv[1] &&
  resolve(process.argv[1]) === fileURLToPath(import.meta.url)
) {
  prepareEnvironmentHelper(process.argv[2] || null)
}
