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
import { verifyWorkerBinary } from "./xinghe-worker-binary.mjs"
import { verifyHelperBinary, helperNames } from "./xinghe-worker-binary.mjs"
import { stageWindowsRuntime } from "./xinghe-worker-msvc.mjs"
import { buildWorker } from "./xinghe-worker-build.mjs"
import { workerCacheKey } from "./xinghe-worker-cache-key.mjs"
import { restoreWorkerCache, saveWorkerCache } from "./xinghe-worker-cache.mjs"

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), "../..")
const WORKER_TARGET_ROOT = join(ROOT, "harness", "xinghe-worker", "target")
const RESOURCE_ROOT = join(ROOT, "src-tauri", "resources", "xinghe-worker")

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
  if (target.includes("windows")) return "iyw_xinghe_worker.dll"
  if (target.includes("apple-darwin")) return "libiyw_xinghe_worker.dylib"
  return "libiyw_xinghe_worker.so"
}

function stageLibrary(source, name) {
  mkdirSync(dirname(RESOURCE_ROOT), { recursive: true })
  const stagingRoot = mkdtempSync(
    join(dirname(RESOURCE_ROOT), ".xinghe-worker-")
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

function verifyBinaries(directory, target) {
  const name = libraryName(target)
  const pin = JSON.parse(
    readFileSync(join(ROOT, "harness", "codex", "upstream.lock"), "utf8")
  )
  verifyWorkerBinary(readFileSync(join(directory, name)), target, pin)
  for (const helper of helperNames(target)) {
    verifyHelperBinary(readFileSync(join(directory, helper)), target)
  }
}

function prepareBinaries(target) {
  const source = join(WORKER_TARGET_ROOT, target, "release")
  if (!existsSync(join(ROOT, ".git"))) {
    console.log("[xinghe-worker] source archive: building without compiler cache")
    buildWorker(ROOT, target)
    verifyBinaries(source, target)
    return source
  }
  const directory = join(WORKER_TARGET_ROOT, "bundle-cache", target)
  const names = [libraryName(target), ...helperNames(target)]
  const key = workerCacheKey(target)
  if (restoreWorkerCache({ directory, key, names })) {
    verifyBinaries(directory, target)
    console.log(`[xinghe-worker] compiler cache hit: ${key}`)
    return directory
  }
  console.log(`[xinghe-worker] compiler cache miss: ${key}`)
  buildWorker(ROOT, target)
  verifyBinaries(source, target)
  saveWorkerCache({ directory, key, names, source })
  return directory
}

function main() {
  const started = performance.now()
  const target = parseTarget(process.argv.slice(2))
  const source = prepareBinaries(target)
  // 复用签名机时，资源目录可能仍有上一架构的 DLL；只清理生成目录。
  rmSync(RESOURCE_ROOT, { recursive: true, force: true })
  for (const name of [libraryName(target), ...helperNames(target)]) {
    stageLibrary(join(source, name), name)
  }
  stageWindowsRuntime({ target, resourceRoot: RESOURCE_ROOT, stageLibrary })
  execFileSync(
    process.execPath,
    [
      join(ROOT, "src-tauri", "scripts", "verify-xinghe-worker-bundle.mjs"),
      "--target",
      target,
    ],
    {
      cwd: ROOT,
      stdio: "inherit",
      windowsHide: true,
    }
  )
  console.log(
    `[xinghe-worker] ready in ${Math.round(performance.now() - started)} ms`
  )
}

if (
  process.argv[1] &&
  resolve(process.argv[1]) === resolve(fileURLToPath(import.meta.url))
) {
  try {
    main()
  } catch (error) {
    console.error(`[xinghe-worker][ERROR] ${error.message}`)
    process.exit(1)
  }
}

export { libraryName, parseTarget }
