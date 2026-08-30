export type AssetReader = (sha256: string, offset: number, length: number) => Promise<{ dataBase64: string; bytes: number }>

type Entry = { url: string; refs: number }

const CHUNK_BYTES = 128 * 1024

export class AssetUrlCache {
  private readonly entries = new Map<string, Entry>()
  private readonly pending = new Map<string, Promise<string>>()
  private disposed = false

  async acquire(sha256: string, bytes: number, mimeType: string, read: AssetReader): Promise<string> {
    if (this.disposed) throw new Error("asset cache is disposed")
    const key = `${sha256}:${mimeType}`
    const cached = this.entries.get(key)
    if (cached) {
      cached.refs += 1
      return cached.url
    }
    const existing = this.pending.get(key)
    const url = existing ?? Promise.resolve().then(() => this.load(sha256, bytes, mimeType, read))
    if (!existing) this.pending.set(key, url)
    let resolved: string
    try { resolved = await url } finally { if (this.pending.get(key) === url) this.pending.delete(key) }
    const entry = this.entries.get(key)
    if (entry) entry.refs += 1
    else this.entries.set(key, { url: resolved, refs: 1 })
    return resolved
  }

  async getOrCreate(sha256: string, bytes: number, mimeType: string, read: AssetReader): Promise<string> {
    if (this.disposed) throw new Error("asset cache is disposed")
    const key = `${sha256}:${mimeType}`
    const cached = this.entries.get(key)
    if (cached) return cached.url
    const pending = this.pending.get(key)
    const url = pending ?? Promise.resolve().then(() => this.load(sha256, bytes, mimeType, read))
    if (!pending) this.pending.set(key, url)
    try {
      const resolved = await url
      if (this.disposed) { URL.revokeObjectURL(resolved); throw new Error("asset cache is disposed") }
      if (!this.entries.has(key)) this.entries.set(key, { url: resolved, refs: 0 })
      return resolved
    } finally { if (this.pending.get(key) === url) this.pending.delete(key) }
  }

  release(sha256: string, mimeType: string): void {
    const key = `${sha256}:${mimeType}`
    const entry = this.entries.get(key)
    if (!entry) return
    entry.refs -= 1
    if (entry.refs > 0) return
    URL.revokeObjectURL(entry.url)
    this.entries.delete(key)
  }

  dispose(): void {
    this.disposed = true
    for (const entry of this.entries.values()) URL.revokeObjectURL(entry.url)
    this.entries.clear()
    this.pending.clear()
  }

  retain(keys: ReadonlySet<string>): void {
    for (const [key, entry] of this.entries) {
      if (keys.has(key)) continue
      URL.revokeObjectURL(entry.url)
      this.entries.delete(key)
    }
  }

  private async load(sha256: string, bytes: number, mimeType: string, read: AssetReader): Promise<string> {
    if (!Number.isSafeInteger(bytes) || bytes < 1) throw new Error("asset size is invalid")
    const parts: Uint8Array[] = []
    for (let offset = 0; offset < bytes; offset += CHUNK_BYTES) {
      const chunk = await read(sha256, offset, CHUNK_BYTES)
      const data = fromBase64(chunk.dataBase64)
      if (!data.byteLength || data.byteLength > CHUNK_BYTES) throw new Error("asset chunk is invalid")
      parts.push(data)
    }
    const actual = parts.reduce((sum, part) => sum + part.byteLength, 0)
    if (actual !== bytes) throw new Error("asset size changed while reading")
    const url = URL.createObjectURL(new Blob(parts, { type: mimeType }))
    if (this.disposed) URL.revokeObjectURL(url)
    return url
  }
}

function fromBase64(value: string): Uint8Array {
  const binary = atob(value)
  return Uint8Array.from(binary, (character) => character.charCodeAt(0))
}
