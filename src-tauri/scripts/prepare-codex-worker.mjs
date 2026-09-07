#!/usr/bin/env node

// 构建桌面应用所需的私有星河动态库，版本由 harness 上游锁定统一控制。

import { execFileSync } from "node:child_process"
import {
  copyFileSync,
  existsSync,
  mkdirSync,
  mkdtempSync,
  renameSync,
  readFileSync,
  rmSync,
} from "node:fs"
import { dirname, join, resolve } from "node:path"
import { fileURLToPath } from "node:url"
import { verifyWorkerBinary } from "./codex-worker-binary.mjs"
import { verifyHelperBinary, helperNames } from "./codex-worker-binary.mjs"

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), "../..")
const WORKER_MANIFEST = join(ROOT, "harness", "codex-worker", "Cargo.toml")
const WORKER_TARGET_ROOT = join(ROOT, "harness", "codex-worker", "target")
const RESOURCE_ROOT = join(ROOT, "src-tauri", "resources", "codex-worker")

function hostTriple() {
  const output = execFileSync("rustc", ["-vV"], {
    encoding: "utf8",
    windowsHide: true,
  })
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
    "--target-dir",
    WORKER_TARGET_ROOT,
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
  const stagingRoot = mkdtempSync(
    join(dirname(RESOURCE_ROOT), ".codex-worker-")
  )
  const stagingFile = join(stagingRoot, name)
  const destination = join(RESOURCE_ROOT, name)
  try {
    copyFileSync(source, stagingFile)
    mkdirSync(RESOURCE_ROOT, { recursive: true })
    if (existsSync(destination)) rmSync(destination, { force: true })
    renameSync(stagingFile, destination)
  } finally {
    rmSync(stagingRoot, { recursive: true, force: true })
  }
  return destination
}

function main() {
  const target = parseTarget(process.argv.slice(2))
  const name = libraryName(target)
  console.log(`[codex-worker] building ${target}`)
  const outputs = ["--lib", "--bin", "iyw-codex-helper"]
  execFileSync("cargo", [...cargoArgs(target), ...outputs], {
    cwd: ROOT,
    stdio: "inherit",
    windowsHide: true,
  })
  const source = builtLibrary(target, name)
  const pin = JSON.parse(
    readFileSync(join(ROOT, "harness", "codex", "upstream.lock"), "utf8")
  )
  verifyWorkerBinary(readFileSync(source), target, pin)
  const destination = stageLibrary(source, name)
  if (target.includes("windows")) {
    execFileSync(
      "cargo",
      [...cargoArgs(target), "-p", "codex-windows-sandbox", "--bins"],
      {
        cwd: ROOT,
        stdio: "inherit",
        windowsHide: true,
      }
    )
  }
  for (const helper of helperNames(target)) {
    const binary = builtLibrary(target, helper)
    verifyHelperBinary(readFileSync(binary), target)
    stageLibrary(binary, helper)
  }
  console.log(`[codex-worker] staged ${destination}`)
  execFileSync(
    process.execPath,
    [
      join(ROOT, "src-tauri", "scripts", "verify-codex-worker-bundle.mjs"),
      "--target",
      target,
    ],
    {
      cwd: ROOT,
      stdio: "inherit",
      windowsHide: true,
    }
  )
  console.log(`[codex-worker] ready for the desktop resource bundle`)
}

if (
  process.argv[1] &&
  resolve(process.argv[1]) === resolve(fileURLToPath(import.meta.url))
) {
  try {
    main()
  } catch (error) {
    console.error(`[codex-worker][ERROR] ${error.message}`)
    process.exit(1)
  }
}

export { libraryName, parseTarget }
