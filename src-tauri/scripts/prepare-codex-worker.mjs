#!/usr/bin/env node

// Experimental only. Builds the private Codex cdylib and stages it for an
// explicit Tauri overlay; the ordinary desktop build never calls this script.

import { execFileSync } from "node:child_process"
import {
  copyFileSync,
  existsSync,
  mkdirSync,
  mkdtempSync,
  renameSync,
  rmSync,
} from "node:fs"
import { dirname, join, resolve } from "node:path"
import { fileURLToPath } from "node:url"

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), "../..")
const WORKER_MANIFEST = join(ROOT, "harness", "codex-worker", "Cargo.toml")
const WORKER_TARGET_ROOT = join(ROOT, "harness", "codex-worker", "target")
const RESOURCE_ROOT = join(ROOT, "src-tauri", "resources", "codex-worker")

function hostTriple() {
  const output = execFileSync("rustc", ["-vV"], { encoding: "utf8" })
  const line = output.split(/\r?\n/).find((value) => value.startsWith("host:"))
  if (!line) throw new Error("rustc -vV did not report a host triple")
  return line.slice("host:".length).trim()
}

function parseTarget(argv) {
  const index = argv.findIndex((value) => value === "--target")
  if (index >= 0 && argv[index + 1]) return argv[index + 1]
  const inline = argv.find((value) => value.startsWith("--target="))
  return (
    inline?.slice("--target=".length) ||
    process.env.TAURI_TARGET_TRIPLE ||
    hostTriple()
  )
}

function libraryName(target) {
  if (target.includes("windows")) return "iyw_codex_worker.dll"
  if (target.includes("apple-darwin")) return "libiyw_codex_worker.dylib"
  return "libiyw_codex_worker.so"
}

function cargoArgs(target) {
  return [
    "build",
    "--manifest-path",
    WORKER_MANIFEST,
    "--release",
    "--locked",
    "--target",
    target,
  ]
}

function builtLibrary(target, name) {
  const targetDir = join(WORKER_TARGET_ROOT, target, "release")
  const path = join(targetDir, name)
  if (!existsSync(path)) {
    throw new Error(`Codex worker build did not produce ${name}`)
  }
  return path
}

function stageLibrary(source, name) {
  mkdirSync(dirname(RESOURCE_ROOT), { recursive: true })
  const stagingRoot = mkdtempSync(join(dirname(RESOURCE_ROOT), ".codex-worker-"))
  const stagingFile = join(stagingRoot, name)
  copyFileSync(source, stagingFile)
  const destination = join(RESOURCE_ROOT, name)
  mkdirSync(RESOURCE_ROOT, { recursive: true })
  // Rename within the same volume after a complete copy. Windows cannot
  // replace an existing file with renameSync, so remove only this staged
  // library before the replacement.
  if (existsSync(destination)) rmSync(destination, { force: true })
  renameSync(stagingFile, destination)
  rmSync(stagingRoot, { recursive: true, force: true })
  return destination
}

function main() {
  const target = parseTarget(process.argv.slice(2))
  const name = libraryName(target)
  console.log(`[codex-worker] building ${target}`)
  execFileSync("cargo", cargoArgs(target), { cwd: ROOT, stdio: "inherit" })
  const destination = stageLibrary(builtLibrary(target, name), name)
  console.log(`[codex-worker] staged ${destination}`)
  console.log(
    `[codex-worker] pass --config src-tauri/tauri.codex-worker.conf.json to an experimental Tauri build`
  )
}

if (process.argv[1] && resolve(process.argv[1]) === resolve(fileURLToPath(import.meta.url))) {
  try {
    main()
  } catch (error) {
    console.error(`[codex-worker][ERROR] ${error.message}`)
    process.exit(1)
  }
}

export { libraryName, parseTarget }
