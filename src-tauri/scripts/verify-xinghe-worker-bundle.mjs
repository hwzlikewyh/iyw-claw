import { existsSync, readFileSync, readdirSync } from "node:fs"
import { dirname, join, resolve } from "node:path"
import { fileURLToPath } from "node:url"
import { parseTarget } from "./prepare-xinghe-worker.mjs"
import {
  verifyHelperBinary,
  windowsRuntimeImports,
} from "./xinghe-worker-binary.mjs"

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), "../..")

export function verifyRuntimeManifest(directory, target) {
  const manifest = JSON.parse(
    readFileSync(join(directory, "runtime.json"), "utf8")
  )
  const pin = JSON.parse(
    readFileSync(join(ROOT, "harness/codex/upstream.lock"), "utf8")
  )
  if (
    manifest.schemaVersion !== 1 ||
    manifest.mode !== "in-process" ||
    manifest.helpers !== "same-executable" ||
    manifest.target !== target ||
    `rust-v${manifest.version}` !== pin.ref ||
    manifest.commit !== pin.commit ||
    manifest.marker !== "IYW_XINGHE_IN_PROCESS_V1"
  )
    throw new Error("embedded Xinghe runtime identity mismatch")
  if (readdirSync(directory).some((name) => name !== "runtime.json"))
    throw new Error("unexpected legacy Xinghe resource in bundle")
  return manifest
}

function executableFor(resourceRoot, target) {
  const windows = target.includes("windows")
  const name = windows ? "iyw-claw.exe" : "iyw-claw"
  const candidates = target.includes("apple-darwin")
    ? [join(resourceRoot, "../MacOS", name)]
    : [
        join(resourceRoot, name),
        join(resourceRoot, "../bin", name),
        join(resourceRoot, "../../bin", name),
      ]
  if (resolve(resourceRoot) === join(ROOT, "src-tauri"))
    candidates.unshift(
      join(ROOT, "src-tauri/target", target, "release", name),
      join(ROOT, "src-tauri/target/release", name)
    )
  const executable = candidates.find(existsSync)
  if (!executable)
    throw new Error(
      "compiled application for embedded Xinghe verification is missing"
    )
  return executable
}

export function verifyWorkerBundle(resourceRoot, target) {
  const directory =
    resolve(resourceRoot) === join(ROOT, "src-tauri")
      ? join(ROOT, "src-tauri/resources/xinghe-worker")
      : join(resourceRoot, "xinghe-resources")
  const manifest = verifyRuntimeManifest(directory, target)
  const executable = executableFor(resourceRoot, target)
  const bytes = readFileSync(executable)
  verifyHelperBinary(bytes, target)
  if (
    !bytes.includes(Buffer.from(manifest.commit)) ||
    !bytes.includes(Buffer.from(manifest.marker))
  )
    throw new Error(
      "application does not contain the expected embedded Xinghe runtime"
    )
  if (target.includes("windows") && windowsRuntimeImports(bytes, target).length)
    throw new Error(
      "same-executable sandbox requires a statically linked MSVC runtime"
    )
  return { ...manifest, executable, size: bytes.length }
}

if (
  process.argv[1] &&
  resolve(process.argv[1]) === fileURLToPath(import.meta.url)
) {
  const target = parseTarget(process.argv.slice(2))
  const value = process.argv.includes("--manifest-only")
    ? verifyRuntimeManifest(
        join(ROOT, "src-tauri/resources/xinghe-worker"),
        target
      )
    : verifyWorkerBundle(join(ROOT, "src-tauri"), target)
  console.log(JSON.stringify(value))
}
