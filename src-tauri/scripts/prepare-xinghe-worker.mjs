#!/usr/bin/env node

import { execFileSync } from "node:child_process"
import { mkdirSync, readFileSync, writeFileSync, readdirSync } from "node:fs"
import { dirname, join, resolve } from "node:path"
import { fileURLToPath } from "node:url"

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), "../..")

export function parseTarget(argv) {
  const index = argv.indexOf("--target")
  const configured = index >= 0 ? argv[index + 1] : undefined
  const inline = argv.find((value) => value.startsWith("--target="))
  const target =
    configured ||
    inline?.slice(9) ||
    process.env.TAURI_TARGET_TRIPLE ||
    execFileSync("rustc", ["-vV"], { encoding: "utf8", windowsHide: true })
      .split(/\r?\n/)
      .find((line) => line.startsWith("host:"))
      ?.slice(5)
      .trim()
  if (!/^[a-z0-9_]+-(?:[a-z0-9_]+-)*[a-z0-9_]+$/.test(target || ""))
    throw new Error("invalid Xinghe runtime target")
  return target
}

export function prepareRuntime(target) {
  const pin = JSON.parse(
    readFileSync(join(ROOT, "harness/codex/upstream.lock"), "utf8")
  )
  const source = readFileSync(
    join(ROOT, "src-tauri/src/internal_xinghe_worker.rs"),
    "utf8"
  )
  const version = source.match(/RUNTIME_VERSION: &str = "([^"]+)"/)?.[1]
  if (pin.ref !== `rust-v${version}`)
    throw new Error("embedded runtime version mismatch")
  const directory = join(ROOT, "src-tauri/resources/xinghe-worker")
  mkdirSync(directory, { recursive: true })
  // 不把工作区遗留的旧 DLL/EXE 混入新包，也不静默删除用户已有文件。
  if (readdirSync(directory).some((name) => name !== "runtime.json"))
    throw new Error(
      "legacy Xinghe binaries remain in generated resources; use a clean staging directory"
    )
  const manifest = {
    schemaVersion: 1,
    mode: "in-process",
    helpers: "same-executable",
    target,
    version,
    commit: pin.commit,
    marker: "IYW_XINGHE_IN_PROCESS_V1",
  }
  writeFileSync(
    join(directory, "runtime.json"),
    JSON.stringify(manifest, null, 2) + "\n"
  )
  console.log(
    `[xinghe-runtime] embedded ${version}, target=${target}; no worker binaries required`
  )
  return manifest
}

if (
  process.argv[1] &&
  resolve(process.argv[1]) === fileURLToPath(import.meta.url)
)
  prepareRuntime(parseTarget(process.argv.slice(2)))
