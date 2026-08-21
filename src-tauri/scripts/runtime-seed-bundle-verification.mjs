import { existsSync, lstatSync, readFileSync } from "node:fs"
import { createHash } from "node:crypto"
import { join, resolve } from "node:path"
import { PINNED_NODE_VERSION } from "./runtime-seed-config.mjs"

function sha256(bytes) {
  return createHash("sha256").update(bytes).digest("hex")
}

function requireFile(path, label) {
  if (
    !existsSync(path) ||
    !lstatSync(path).isFile() ||
    lstatSync(path).size === 0
  )
    throw new Error(`${label} is missing or empty: ${path}`)
}

export function verifyInstalledRuntimeSeed(appDirectory, target, die) {
  const seedRoot = join(appDirectory, "runtime-seed")
  if (target === "i686-pc-windows-msvc") {
    if (existsSync(seedRoot))
      die(`Windows x86 must not install runtime seed: ${seedRoot}`)
    return
  }
  const manifestPath = join(seedRoot, "manifest.json")
  requireFile(manifestPath, "installed runtime seed manifest")
  const manifest = JSON.parse(readFileSync(manifestPath, "utf8"))
  const node = manifest.components?.find((component) => component.id === "node")
  if (
    manifest.schemaVersion !== 2 ||
    manifest.target !== target ||
    manifest.components?.length !== 4 ||
    node?.version !== PINNED_NODE_VERSION
  ) {
    die(`installed runtime seed does not match ${target}: ${manifestPath}`)
  }
  for (const component of manifest.components) {
    const archive = resolve(seedRoot, component.archive ?? "")
    if (!archive.startsWith(`${resolve(seedRoot)}\\`))
      die(`installed runtime seed archive escaped resource root: ${archive}`)
    requireFile(archive, `installed ${component.id} runtime seed`)
    const metadata = lstatSync(archive)
    if (
      metadata.size !== component.archiveSize ||
      sha256(readFileSync(archive)) !== component.archiveSha256
    )
      die(`installed runtime seed archive is invalid: ${archive}`)
  }
}
