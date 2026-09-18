import { createHash } from "node:crypto"
import {
  copyFileSync,
  existsSync,
  mkdirSync,
  readFileSync,
  writeFileSync,
} from "node:fs"
import { join } from "node:path"

const MANIFEST = "manifest.json"

function digest(path) {
  return createHash("sha256").update(readFileSync(path)).digest("hex")
}

export function restoreWorkerCache({ directory, key, legacyKey, names }) {
  const path = join(directory, MANIFEST)
  if (!existsSync(path)) return false
  const manifest = JSON.parse(readFileSync(path, "utf8"))
  if (manifest.key !== key && (!legacyKey || manifest.key !== legacyKey)) {
    console.warn(
      `[xinghe-worker] cache identity mismatch: expected ${key}, found ${manifest.key}`
    )
    return false
  }
  for (const name of names) {
    if (manifest.files?.[name] !== digest(join(directory, name))) {
      throw new Error(`worker cache integrity failed: ${name}`)
    }
  }
  if (manifest.key !== key) {
    // 仅迁移与当前完整旧指纹匹配的缓存，不接受其他镜像下无法核验的旧产物。
    writeFileSync(path, JSON.stringify({ key, files: manifest.files }))
    console.log(
      `[xinghe-worker] migrated verified cache: ${manifest.key} -> ${key}`
    )
  }
  return true
}

export function saveWorkerCache({ directory, key, names, source }) {
  mkdirSync(directory, { recursive: true })
  const files = {}
  for (const name of names) {
    copyFileSync(join(source, name), join(directory, name))
    files[name] = digest(join(directory, name))
  }
  // 最后写入清单，失败构建不能被当作完整缓存复用。
  writeFileSync(join(directory, MANIFEST), JSON.stringify({ key, files }))
}
