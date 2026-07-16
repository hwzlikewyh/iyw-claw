import { existsSync, rmSync } from "node:fs"
import { dirname, resolve } from "node:path"
import { fileURLToPath } from "node:url"

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..")
const artifacts = [".next", "out", "coverage", "src-tauri/target"]

for (const relativePath of artifacts) {
  const path = resolve(root, relativePath)
  if (!existsSync(path)) continue

  rmSync(path, { force: true, maxRetries: 3, recursive: true, retryDelay: 100 })
  console.log(`Removed ${relativePath}`)
}
