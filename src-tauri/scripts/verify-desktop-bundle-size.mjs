import { readdir, stat } from "node:fs/promises"
import { join, resolve } from "node:path"
import { fileURLToPath } from "node:url"
import { parseTarget, targetInfo } from "./runtime-seed-config.mjs"

const ROOT = resolve(fileURLToPath(new URL("../..", import.meta.url)))
const MIB = 1024 * 1024
// Seed-enabled bundles contain four already-compressed runtime archives, so
// their final installers need a higher ceiling than the online-only package.
const BUDGETS = {
  "x86_64-pc-windows-msvc": 620,
  "i686-pc-windows-msvc": 80,
  "x86_64-apple-darwin": 620,
  "aarch64-apple-darwin": 600,
  "x86_64-unknown-linux-gnu": 700,
  "aarch64-unknown-linux-gnu": 650,
}

async function files(root, predicate, result = []) {
  for (const entry of await readdir(root, { withFileTypes: true })) {
    const path = join(root, entry.name)
    if (entry.isDirectory()) await files(path, predicate, result)
    else if (predicate(entry.name)) result.push(path)
  }
  return result
}

function bundleRoot(target) {
  return join(ROOT, "src-tauri", "target", target, "release", "bundle")
}

function extensions(target) {
  if (target.includes("windows")) return [".exe"]
  if (target.includes("apple")) return [".dmg", ".tar.gz"]
  return [".AppImage", ".deb", ".rpm"]
}

async function verifyBundleSize(target = parseTarget()) {
  const info = targetInfo(target)
  const root = bundleRoot(target)
  const allowed = extensions(target)
  const artifacts = await files(root, (name) =>
    allowed.some((suffix) => name.endsWith(suffix))
  )
  if (artifacts.length === 0)
    throw new Error(`no final desktop artifacts found under ${root}`)
  const budget = BUDGETS[target] * MIB
  for (const artifact of artifacts) {
    const size = (await stat(artifact)).size
    console.log(
      `[desktop-size] ${target} ${artifact} ${(size / MIB).toFixed(1)} MiB / ${BUDGETS[target]} MiB`
    )
    if (size > budget)
      throw new Error(
        `desktop artifact exceeds ${BUDGETS[target]} MiB budget: ${artifact}`
      )
  }
  if (
    info.skipped &&
    artifacts.some((artifact) =>
      artifact.toLowerCase().includes("runtime-seed")
    )
  ) {
    throw new Error("Windows x86 artifact unexpectedly contains runtime seed")
  }
}

const target = parseTarget()
if (
  process.argv[1] &&
  resolve(process.argv[1]) === resolve(fileURLToPath(import.meta.url))
) {
  verifyBundleSize(target).catch((error) => {
    console.error(`[desktop-size] ${error.message}`)
    process.exitCode = 1
  })
}

export { verifyBundleSize }
