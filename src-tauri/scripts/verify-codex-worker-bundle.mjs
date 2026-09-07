import { createHash } from "node:crypto"
import { existsSync, readFileSync } from "node:fs"
import { dirname, join, resolve } from "node:path"
import { fileURLToPath } from "node:url"
import { libraryName, parseTarget } from "./prepare-codex-worker.mjs"
import {
  verifyWorkerBinary,
  verifyHelperBinary,
  helperNames,
} from "./codex-worker-binary.mjs"

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), "../..")

function bytes(path) {
  if (!existsSync(path)) throw new Error(`built-in worker is missing: ${path}`)
  const value = readFileSync(path)
  if (value.length === 0) throw new Error(`built-in worker is empty: ${path}`)
  return value
}

function digest(value) {
  return createHash("sha256").update(value).digest("hex")
}

export function verifyWorkerBundle(resourceRoot, target, compareStaged = true) {
  const name = libraryName(target)
  const sourceRoot = join(ROOT, "src-tauri", "resources", "codex-worker")
  const installedRoot =
    resolve(resourceRoot) === join(ROOT, "src-tauri")
      ? sourceRoot
      : join(resourceRoot, "codex-resources")
  const value = bytes(join(installedRoot, name))
  const expected = JSON.parse(
    readFileSync(join(ROOT, "harness", "codex", "upstream.lock"), "utf8")
  )
  const loader = readFileSync(
    join(ROOT, "src-tauri", "src", "internal_codex_worker.rs"),
    "utf8"
  )
  const version = loader.match(/RUNTIME_VERSION: &str = "([^"]+)"/)?.[1]
  if (expected.ref !== `rust-v${version}`)
    throw new Error("worker and desktop runtime versions disagree")
  verifyWorkerBinary(value, target, expected)
  for (const helper of helperNames(target)) {
    const binary = bytes(join(installedRoot, helper))
    verifyHelperBinary(binary, target)
    if (
      compareStaged &&
      digest(binary) !== digest(bytes(join(sourceRoot, helper)))
    )
      throw new Error(
        `installed sandbox helper differs from staging: ${helper}`
      )
  }
  if (compareStaged) {
    const staged = bytes(
      join(ROOT, "src-tauri", "resources", "codex-worker", name)
    )
    if (digest(value) !== digest(staged))
      throw new Error("installed worker differs from the staged library")
  }
  return { library: name, version, sha256: digest(value), size: value.length }
}

if (
  process.argv[1] &&
  resolve(process.argv[1]) === fileURLToPath(import.meta.url)
) {
  const target = parseTarget(process.argv.slice(2))
  console.log(
    JSON.stringify(verifyWorkerBundle(join(ROOT, "src-tauri"), target, false))
  )
}
