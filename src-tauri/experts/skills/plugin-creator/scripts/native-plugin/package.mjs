import { readdir, stat } from "node:fs/promises"
import path from "node:path"

const MAX_FILES = 512
const MAX_BYTES = 25 * 1024 * 1024
const MAX_PATH_BYTES = 512

export async function packageStats(root) {
  const known = new Set()
  let files = 0
  let bytes = 0
  const visit = async (directory, prefix = "") => {
    for (const entry of await readdir(directory, { withFileTypes: true })) {
      const relative = prefix ? `${prefix}/${entry.name}` : entry.name
      const folded = relative.toLowerCase()
      if (known.has(folded) || Buffer.byteLength(relative) > MAX_PATH_BYTES)
        throw Error(`Invalid or duplicate package path: ${relative}`)
      known.add(folded)
      const file = path.join(directory, entry.name)
      if (entry.isDirectory()) await visit(file, relative)
      else if (entry.isFile()) {
        files++
        bytes += (await stat(file)).size
        if (files > MAX_FILES || bytes > MAX_BYTES)
          throw Error("Generated plugin exceeds market package limits")
      } else throw Error(`Unsupported package entry: ${relative}`)
    }
  }
  await visit(root)
  return { files, bytes }
}
