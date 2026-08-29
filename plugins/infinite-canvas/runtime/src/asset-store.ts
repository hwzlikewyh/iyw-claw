import { createHash, randomUUID } from "node:crypto"
import { mkdir, open, readFile, rename, stat, readdir, unlink, copyFile } from "node:fs/promises"
import { createReadStream } from "node:fs"
import { basename, extname, join } from "node:path"
import { CanvasRuntimeError, invalid } from "./errors.js"
import { assertWithin, pluginDataRoot, rejectSymlink, storageRoot, workspacePath } from "./paths.js"
import type { AssetRef } from "./types.js"

const CHUNK_BYTES = 128 * 1024
const SHA_PATTERN = /^[a-f0-9]{64}$/

type Upload = { id: string; path: string; name: string; mimeType: string; expectedBytes: number; expectedSha256: string; nextIndex: number; bytes: number }

export class AssetStore {
  private readonly uploads = new Map<string, Upload>()

  async begin(name: string, mimeType: string, expectedBytes: number, expectedSha256: string): Promise<{ uploadId: string }> {
    if (!Number.isSafeInteger(expectedBytes) || expectedBytes < 1 || !SHA_PATTERN.test(expectedSha256)) throw invalid("asset_upload_metadata_invalid")
    const id = randomUUID()
    const root = join(pluginDataRoot(), "uploads")
    await mkdir(root, { recursive: true })
    const path = join(root, `${id}.part`)
    const handle = await open(path, "wx")
    await handle.close()
    this.uploads.set(id, { id, path, name, mimeType, expectedBytes, expectedSha256, nextIndex: 0, bytes: 0 })
    return { uploadId: id }
  }

  async writeChunk(uploadId: string, chunkIndex: number, dataBase64: string): Promise<{ bytes: number; nextIndex: number }> {
    const upload = this.uploads.get(uploadId)
    if (!upload || !Number.isInteger(chunkIndex) || chunkIndex !== upload.nextIndex) throw new CanvasRuntimeError("asset_upload_invalid", "upload chunk index is invalid")
    const data = decodeChunk(dataBase64)
    if (upload.bytes + data.byteLength > upload.expectedBytes) throw new CanvasRuntimeError("asset_upload_invalid", "asset exceeds declared size")
    const handle = await open(upload.path, "a")
    try { await handle.write(data) } finally { await handle.close() }
    upload.bytes += data.byteLength
    upload.nextIndex += 1
    return { bytes: upload.bytes, nextIndex: upload.nextIndex }
  }

  async finalize(uploadId: string): Promise<AssetRef> {
    const upload = this.uploads.get(uploadId)
    if (!upload) throw new CanvasRuntimeError("asset_upload_invalid", "upload is not active")
    if (upload.bytes !== upload.expectedBytes) throw new CanvasRuntimeError("asset_upload_incomplete", "asset size is incomplete", { bytes: upload.bytes, expectedBytes: upload.expectedBytes })
    const hash = await hashFile(upload.path)
    if (hash !== upload.expectedSha256) {
      await this.discard(upload)
      throw new CanvasRuntimeError("asset_hash_mismatch", "asset hash does not match", { expectedSha256: upload.expectedSha256 })
    }
    const target = await this.uniqueAssetPath(hash, upload.name)
    await mkdir(join(storageRoot(), "assets"), { recursive: true })
    await rename(upload.path, target)
    this.uploads.delete(uploadId)
    return { sha256: hash, mimeType: upload.mimeType, bytes: upload.bytes, path: this.relativeAssetPath(target) }
  }

  async importSource(sourcePath: string, name: string, mimeType: string, expectedSha256?: string): Promise<AssetRef> {
    const source = workspacePath(sourcePath)
    await rejectSymlink(source)
    const info = await stat(source)
    if (!info.isFile()) throw invalid("asset_source_not_file")
    const hash = await hashFile(source)
    if (expectedSha256 && hash !== expectedSha256) throw new CanvasRuntimeError("asset_hash_mismatch", "source asset hash does not match")
    await mkdir(join(storageRoot(), "assets"), { recursive: true })
    const upload = await this.begin(name || basename(source), mimeType, info.size, hash)
    const state = this.uploads.get(upload.uploadId)
    if (!state) throw new CanvasRuntimeError("asset_upload_invalid", "source upload is not active")
    await copyFile(source, state.path)
    state.bytes = info.size
    return this.finalize(upload.uploadId)
  }

  async readSourceText(sourcePath: string, maxBytes = 200_000): Promise<string> {
    const source = workspacePath(sourcePath)
    await rejectSymlink(source)
    const info = await stat(source)
    if (!info.isFile() || info.size > maxBytes) throw invalid("asset_source_text_invalid")
    return readFile(source, "utf8")
  }

  async readChunk(sha256: string, offset: number, length: number): Promise<{ dataBase64: string; bytes: number; eof: boolean }> {
    if (!SHA_PATTERN.test(sha256) || !Number.isInteger(offset) || !Number.isInteger(length) || offset < 0 || length < 1 || length > CHUNK_BYTES) throw invalid("asset_read_range_invalid")
    const path = await this.findAsset(sha256)
    const data = await readFile(path)
    const slice = data.subarray(offset, offset + length)
    return { dataBase64: slice.toString("base64"), bytes: data.length, eof: offset + slice.length >= data.length }
  }

  async close(): Promise<void> {
    await Promise.all([...this.uploads.values()].map((upload) => unlink(upload.path).catch(() => undefined)))
    this.uploads.clear()
  }

  private async discard(upload: Upload): Promise<void> {
    this.uploads.delete(upload.id)
    await unlink(upload.path).catch(() => undefined)
  }

  private async findAsset(sha256: string): Promise<string> {
    const entries = await readdir(join(storageRoot(), "assets")).catch((error: unknown) => {
      if ((error as NodeJS.ErrnoException).code === "ENOENT") throw new CanvasRuntimeError("asset_not_found", "asset was not found")
      throw error
    })
    const name = entries.find((entry) => entry.startsWith(`${sha256}.`))
    if (!name) throw new CanvasRuntimeError("asset_not_found", "asset was not found")
    return assertWithin(join(storageRoot(), "assets"), join(storageRoot(), "assets", name))
  }

  private async uniqueAssetPath(hash: string, name: string): Promise<string> {
    const extension = normalizeExtension(name)
    const path = join(storageRoot(), "assets", `${hash}${extension}`)
    try { await stat(path); return path } catch (error) { if ((error as NodeJS.ErrnoException).code === "ENOENT") return path; throw error }
  }

  private relativeAssetPath(path: string): string {
    return path.slice(storageRoot().length + 1).replaceAll("\\", "/")
  }
}

function normalizeExtension(name: string): string {
  const extension = extname(name).toLowerCase().replace(/[^a-z0-9.]/g, "")
  return extension.length > 1 && extension.length <= 12 ? extension : ".bin"
}

function decodeChunk(dataBase64: string): Buffer {
  if (!/^[A-Za-z0-9+/]+={0,2}$/.test(dataBase64) || dataBase64.length % 4 === 1) throw invalid("asset_chunk_base64_invalid")
  const data = Buffer.from(dataBase64, "base64")
  if (data.length > CHUNK_BYTES) throw invalid("asset_chunk_too_large")
  return data
}

async function hashFile(path: string): Promise<string> {
  const hash = createHash("sha256")
  for await (const chunk of createReadStream(path)) hash.update(chunk)
  return hash.digest("hex")
}
