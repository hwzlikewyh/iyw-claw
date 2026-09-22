#!/usr/bin/env node

import { execFileSync } from "node:child_process"
import {
  copyFileSync,
  existsSync,
  mkdirSync,
  readdirSync,
  rmSync,
} from "node:fs"
import { dirname, join, resolve } from "node:path"
import { fileURLToPath } from "node:url"
import process from "node:process"
import {
  environmentHelperBuildOptions,
  environmentHelperHostTarget,
  helperExecutableName,
  helperFileName,
  verifyEnvironmentRuntime,
} from "./environment-helper-runtime.mjs"

const SRC_TAURI = resolve(dirname(fileURLToPath(import.meta.url)), "..")
const MANIFEST = join(SRC_TAURI, "environment-core", "Cargo.toml")
const TARGET_DIR = join(SRC_TAURI, "target", "environment-helper")
const BINARIES = join(SRC_TAURI, "binaries")

export function prepareEnvironmentHelper(target = null) {
  const resolvedTarget =
    target || process.env.TAURI_TARGET_TRIPLE || environmentHelperHostTarget()
  const options = environmentHelperBuildOptions(resolvedTarget)
  execFileSync(
    "cargo",
    [
      "build",
      "--release",
      "--manifest-path",
      MANIFEST,
      "--target-dir",
      TARGET_DIR,
      "--target",
      resolvedTarget,
      ...options.args,
    ],
    { cwd: SRC_TAURI, stdio: "inherit", env: options.env }
  )
  const source = join(
    TARGET_DIR,
    resolvedTarget,
    "release",
    helperExecutableName(resolvedTarget)
  )
  if (!existsSync(source))
    throw new Error(`environment helper missing: ${source}`)
  verifyEnvironmentRuntime(source, resolvedTarget)
  mkdirSync(BINARIES, { recursive: true })
  for (const entry of readdirSync(BINARIES)) {
    if (/^iyw-environment-/.test(entry)) rmSync(join(BINARIES, entry))
  }
  const destination = join(BINARIES, helperFileName(resolvedTarget))
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
