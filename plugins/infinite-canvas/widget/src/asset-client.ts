import { AssetUrlCache, type AssetReader } from "./asset-url-cache.js"

export type AssetReference = { sha256: string; bytes: number; mimeType: string; path?: string }
export type AssetToolCaller = (name: string, args: Record<string, unknown>) => Promise<unknown>

const CHUNK_BYTES = 128 * 1024

export class AssetClient {
  private readonly urls = new AssetUrlCache()

  constructor(private readonly call: AssetToolCaller) {}

  async upload(blob: Blob, name: string, mimeType = blob.type || "application/octet-stream"): Promise<AssetReference> {
    const bytes = new Uint8Array(await blob.arrayBuffer())
    if (!bytes.byteLength) throw new Error("asset is empty")
    const expectedSha256 = await digest(bytes)
    const started = await this.call("write_infinite_canvas_asset", { name, mimeType, expectedBytes: bytes.byteLength, expectedSha256 }) as { uploadId?: string }
    if (typeof started.uploadId !== "string") throw new Error("asset upload did not start")
    try {
      await this.writeChunks(started.uploadId, bytes)
      return await this.call("write_infinite_canvas_asset", { uploadId: started.uploadId, finalize: true }) as AssetReference
    } catch (error) {
      await this.call("write_infinite_canvas_asset", { uploadId: started.uploadId, cancel: true }).catch(() => undefined)
      throw error
    }
  }

  async importSource(sourcePath: string, name: string, mimeType: string): Promise<AssetReference> {
    return this.call("write_infinite_canvas_asset", { sourcePath, name, mimeType }) as Promise<AssetReference>
  }

  async getUrl(reference: AssetReference): Promise<string> {
    const read: AssetReader = (sha256, offset, length) => this.call("read_infinite_canvas_asset", { sha256, offset, length }) as Promise<{ dataBase64: string; bytes: number }>
    return this.urls.getOrCreate(reference.sha256, reference.bytes, reference.mimeType, read)
  }

  release(reference: AssetReference): void { this.urls.release(reference.sha256, reference.mimeType) }
  retain(references: AssetReference[]): void { this.urls.retain(new Set(references.map((reference) => `${reference.sha256}:${reference.mimeType}`))) }
  dispose(): void { this.urls.dispose() }

  private async writeChunks(uploadId: string, bytes: Uint8Array): Promise<void> {
    for (let offset = 0, chunkIndex = 0; offset < bytes.length; offset += CHUNK_BYTES, chunkIndex += 1) {
      const dataBase64 = toBase64(bytes.subarray(offset, offset + CHUNK_BYTES))
      await this.call("write_infinite_canvas_asset", { uploadId, chunkIndex, dataBase64 })
    }
  }
}

async function digest(bytes: Uint8Array): Promise<string> {
  const value = await crypto.subtle.digest("SHA-256", bytes)
  return [...new Uint8Array(value)].map((byte) => byte.toString(16).padStart(2, "0")).join("")
}

function toBase64(bytes: Uint8Array): string {
  let value = ""
  for (let offset = 0; offset < bytes.length; offset += 0x8000) value += String.fromCharCode(...bytes.subarray(offset, offset + 0x8000))
  return btoa(value)
}
