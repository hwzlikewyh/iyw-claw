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

export function restoreWorkerCache({ directory, key, names }) {
  const path = join(directory, MANIFEST)
  if (!existsSync(path)) return false
  const manifest = JSON.parse(readFileSync(path, "utf8"))
  if (manifest.key !== key) return false
  for (const name of names) {
    if (manifest.files?.[name] !== digest(join(directory, name))) {
      throw new Error(`worker cache integrity failed: ${name}`)
    }
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
