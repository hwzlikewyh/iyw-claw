import type { Transport } from "./transport"

const SOURCE_MAX_BYTES = 4 * 1024 * 1024
const CACHE_MAX_CHARS = 8 * 1024 * 1024
const CACHE_MAX_ENTRIES = 64
const CACHE_MAX_PIXELS = 4 * 1024 * 1024
const MAX_DECODERS = 2
const MIME_TYPES: Record<string, string> = {
  bmp: "image/bmp",
  gif: "image/gif",
  ico: "image/x-icon",
  jpeg: "image/jpeg",
  jpg: "image/jpeg",
  png: "image/png",
  svg: "image/svg+xml",
  webp: "image/webp",
}

export interface LocalThumbnailRequest {
  path: string
  rootPath?: string
  revision: string
  edge: number
  fit: "cover" | "contain"
}

interface PendingThumbnail {
  promise: Promise<string>
  controller: AbortController
  consumers: number
}

interface ThumbnailCache {
  values: Map<string, { source: string; pixels: number }>
  pending: Map<string, PendingThumbnail>
  chars: number
  pixels: number
}

const caches = new WeakMap<Transport, ThumbnailCache>()
const waiting: Array<() => void> = []
let decoders = 0

export function loadLocalArtifactThumbnail(
  transport: Transport,
  request: LocalThumbnailRequest,
  signal: AbortSignal
): Promise<string> {
  let cache = caches.get(transport)
  if (!cache) {
    cache = { values: new Map(), pending: new Map(), chars: 0, pixels: 0 }
    caches.set(transport, cache)
  }
  const key = JSON.stringify(request)
  const cached = cache.values.get(key)
  if (cached !== undefined) {
    cache.values.delete(key)
    cache.values.set(key, cached)
    return Promise.resolve(cached.source)
  }
  let pending = cache.pending.get(key)
  if (!pending) {
    pending = startThumbnail(cache, transport, request)
    cache.pending.set(key, pending)
  }
  return subscribeThumbnail(pending, signal, () => {
    if (cache.pending.get(key) === pending) cache.pending.delete(key)
  })
}

function startThumbnail(
  target: ThumbnailCache,
  transport: Transport,
  request: LocalThumbnailRequest
): PendingThumbnail {
  const key = JSON.stringify(request)
  const controller = new AbortController()
  const pending: PendingThumbnail = {
    controller,
    consumers: 0,
    promise: produceThumbnail(transport, request, controller.signal)
      .then((source) => {
        controller.signal.throwIfAborted()
        if (source.cacheable)
          remember(target, key, {
            source: source.src,
            pixels: request.edge * request.edge,
          })
        return source.src
      })
      .finally(() => {
        if (target.pending.get(key) === pending) target.pending.delete(key)
      }),
  }
  return pending
}

function subscribeThumbnail(
  pending: PendingThumbnail,
  signal: AbortSignal,
  remove: () => void
): Promise<string> {
  pending.consumers += 1
  let released = false
  const release = () => {
    if (released) return
    released = true
    signal.removeEventListener("abort", release)
    pending.consumers -= 1
    if (pending.consumers === 0) {
      remove()
      pending.controller.abort()
    }
  }
  signal.addEventListener("abort", release, { once: true })
  if (signal.aborted) release()
  return pending.promise.finally(release)
}

function remember(
  cache: ThumbnailCache,
  key: string,
  entry: { source: string; pixels: number }
): void {
  const { source, pixels } = entry
  if (source.length > CACHE_MAX_CHARS) return
  cache.values.set(key, entry)
  cache.chars += source.length
  cache.pixels += pixels
  while (
    cache.chars > CACHE_MAX_CHARS ||
    cache.values.size > CACHE_MAX_ENTRIES ||
    cache.pixels > CACHE_MAX_PIXELS
  ) {
    const oldest = cache.values.entries().next().value
    if (!oldest) break
    cache.values.delete(oldest[0])
    cache.chars -= oldest[1].source.length
    cache.pixels -= oldest[1].pixels
  }
}

async function acquireDecoder(signal: AbortSignal): Promise<void> {
  signal.throwIfAborted()
  if (decoders < MAX_DECODERS) {
    decoders += 1
    return
  }
  await new Promise<void>((resolve, reject) => {
    const next = () => {
      signal.removeEventListener("abort", abort)
      resolve()
    }
    const abort = () => {
      const index = waiting.indexOf(next)
      if (index >= 0) waiting.splice(index, 1)
      reject(signal.reason)
    }
    signal.addEventListener("abort", abort, { once: true })
    waiting.push(next)
  })
}

function releaseDecoder(): void {
  const next = waiting.shift()
  if (next) next()
  else decoders -= 1
}

async function produceThumbnail(
  transport: Transport,
  request: LocalThumbnailRequest,
  signal: AbortSignal
): Promise<{ src: string; cacheable: boolean }> {
  await acquireDecoder(signal)
  try {
    signal.throwIfAborted()
    const base64 = await transport.call<string>(
      request.rootPath ? "read_workspace_file_base64" : "read_file_base64",
      {
        path: request.path,
        rootPath: request.rootPath,
        maxBytes: SOURCE_MAX_BYTES,
      }
    )
    signal.throwIfAborted()
    const extension = request.path.split(".").pop()?.toLowerCase() ?? ""
    const mime = MIME_TYPES[extension] ?? "application/octet-stream"
    const original = `data:${mime};base64,${base64}`
    const unchanged = { src: original, cacheable: false }
    if (!/^(png|jpe?g)$/.test(extension)) return unchanged
    const bytes = Uint8Array.from(atob(base64), (char) => char.charCodeAt(0))
    if (extension === "png" && isAnimatedPng(bytes)) return unchanged
    return await resizeImage(new Blob([bytes], { type: mime }), request, signal)
      .then((src) => ({ src, cacheable: true }))
      .catch(() => unchanged)
  } finally {
    releaseDecoder()
  }
}

function isAnimatedPng(bytes: Uint8Array): boolean {
  const signatureBytes = 8
  const chunkOverhead = 12
  const animationControl = 0x6163544c
  const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength)
  let offset = signatureBytes
  while (offset + chunkOverhead <= bytes.length) {
    if (view.getUint32(offset + 4) === animationControl) return true
    const size = view.getUint32(offset)
    offset += size + chunkOverhead
  }
  return false
}

async function resizeImage(
  blob: Blob,
  request: LocalThumbnailRequest,
  signal: AbortSignal
): Promise<string> {
  const bitmap = await createImageBitmap(blob)
  const canvas = document.createElement("canvas")
  try {
    signal.throwIfAborted()
    const cover = request.fit === "cover"
    const side = Math.min(bitmap.width, bitmap.height)
    const width = cover ? side : bitmap.width
    const height = cover ? side : bitmap.height
    const scale = Math.min(1, request.edge / Math.max(width, height))
    canvas.width = Math.max(1, Math.round(width * scale))
    canvas.height = Math.max(1, Math.round(height * scale))
    const context = canvas.getContext("2d")
    if (!context) throw new Error("Image canvas unavailable")
    context.imageSmoothingEnabled = true
    context.imageSmoothingQuality = "high"
    context.drawImage(
      bitmap,
      (bitmap.width - width) / 2,
      (bitmap.height - height) / 2,
      width,
      height,
      0,
      0,
      canvas.width,
      canvas.height
    )
    return canvas.toDataURL("image/png")
  } finally {
    bitmap.close()
    canvas.width = 0
    canvas.height = 0
  }
}
