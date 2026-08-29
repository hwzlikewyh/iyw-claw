import { createHash } from "node:crypto"
import { lstat, mkdir, readFile, stat, writeFile } from "node:fs/promises"
import { join, relative, resolve } from "node:path"
import { fileURLToPath } from "node:url"

const root = fileURLToPath(new URL("..", import.meta.url))
const output = join(root, "dist/infinite-canvas-0.1.8.zip")
const included = [".iyw-plugin.json", "runtime/dist/infinite-canvas-mcp.mjs", "widget/dist/infinite-canvas-widget.html", "contracts", "skills", "LICENSE", "THIRD_PARTY_NOTICES.md", "upstream.json", "dist/license-report.json"]
const files = await collect(included)
files.sort((left, right) => left.path.localeCompare(right.path))
const chunks = []
const central = []
let offset = 0
for (const file of files) {
  const name = Buffer.from(file.path, "utf8")
  const crc = crc32(file.data)
  const header = Buffer.alloc(30)
  header.writeUInt32LE(0x04034b50, 0)
  header.writeUInt16LE(20, 4)
  header.writeUInt16LE(0x800, 6)
  header.writeUInt16LE(0, 8)
  header.writeUInt16LE(0, 10)
  header.writeUInt16LE(33, 12)
  header.writeUInt32LE(crc, 14)
  header.writeUInt32LE(file.data.length, 18)
  header.writeUInt32LE(file.data.length, 22)
  header.writeUInt16LE(name.length, 26)
  header.writeUInt16LE(0, 28)
  chunks.push(header, name, file.data)
  const entry = Buffer.alloc(46)
  entry.writeUInt32LE(0x02014b50, 0)
  entry.writeUInt16LE(20, 4)
  entry.writeUInt16LE(20, 6)
  entry.writeUInt16LE(0x800, 8)
  entry.writeUInt16LE(0, 10)
  entry.writeUInt16LE(0, 12)
  entry.writeUInt16LE(33, 14)
  entry.writeUInt32LE(crc, 16)
  entry.writeUInt32LE(file.data.length, 20)
  entry.writeUInt32LE(file.data.length, 24)
  entry.writeUInt16LE(name.length, 28)
  entry.writeUInt16LE(0, 30)
  entry.writeUInt16LE(0, 32)
  entry.writeUInt16LE(0, 34)
  entry.writeUInt16LE(0, 36)
  entry.writeUInt32LE(0, 38)
  entry.writeUInt32LE(offset, 42)
  central.push(entry, name)
  offset += header.length + name.length + file.data.length
}
const centralBytes = Buffer.concat(central)
const end = Buffer.alloc(22)
end.writeUInt32LE(0x06054b50, 0)
end.writeUInt16LE(files.length, 8)
end.writeUInt16LE(files.length, 10)
end.writeUInt32LE(centralBytes.length, 12)
end.writeUInt32LE(offset, 16)
const zip = Buffer.concat([...chunks, centralBytes, end])
await mkdir(join(root, "dist"), { recursive: true })
await writeFile(output, zip)
console.log(JSON.stringify({ output, files: files.length, bytes: zip.length, sha256: createHash("sha256").update(zip).digest("hex") }, null, 2))

async function collect(entries) {
  const result = []
  for (const entry of entries) {
    const absolute = join(root, entry)
    const info = await lstat(absolute)
    if (info.isSymbolicLink()) throw new Error(`symbolic link is not allowed: ${entry}`)
    if (info.isDirectory()) result.push(...await collectDirectory(absolute))
    else result.push({ path: entry.replaceAll("\\", "/"), data: await readFile(absolute) })
  }
  return result
}
async function collectDirectory(directory) {
  const result = []
  for (const name of (await (await import("node:fs/promises")).readdir(directory))) result.push(...await collect([relative(root, join(directory, name))]))
  return result
}
function crc32(data) { let crc = 0xffffffff; for (const byte of data) { crc ^= byte; for (let bit = 0; bit < 8; bit += 1) crc = (crc >>> 1) ^ (crc & 1 ? 0xedb88320 : 0) } return (crc ^ 0xffffffff) >>> 0 }
