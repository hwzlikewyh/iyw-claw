import type { PreviewState } from "@/components/message/workspace-file-preview"
import { toImageDataUrl } from "@/components/message/workspace-file-preview"
import { readFilePreview, readWorkspaceFileBase64 } from "@/lib/api"
import { isImageFile } from "@/lib/language-detect"

const PREVIEW_CACHE_TTL_MS = 2_000
const PREVIEW_CACHE_MAX_ENTRIES = 8
const PREVIEW_CACHE_MAX_CHARS = 8_000_000
const TEXT_PREVIEW_MAX_BYTES = 2 * 1024 * 1024

export type CacheablePreview = Extract<
  PreviewState,
  { status: "image" | "text" }
>

interface CachedPreview {
  expiresAt: number
  preview: CacheablePreview
}

const previewCache = new Map<string, CachedPreview>()
const previewRequests = new Map<string, Promise<CacheablePreview>>()
let cachedCharacters = 0

function previewKey(rootPath: string, path: string): string {
  return `${rootPath}\0${path}`
}

function deleteCachedPreview(key: string): void {
  const cached = previewCache.get(key)
  if (!cached) return
  cachedCharacters -= cached.preview.content.length
  previewCache.delete(key)
}

export function getCachedWorkspacePreview(
  rootPath: string,
  path: string
): CacheablePreview | null {
  const key = previewKey(rootPath, path)
  const cached = previewCache.get(key)
  if (!cached) return null
  if (cached.expiresAt <= Date.now()) {
    deleteCachedPreview(key)
    return null
  }
  previewCache.delete(key)
  previewCache.set(key, cached)
  return cached.preview
}

function cachePreview(key: string, preview: CacheablePreview): void {
  deleteCachedPreview(key)
  if (preview.content.length > PREVIEW_CACHE_MAX_CHARS / 2) return
  previewCache.set(key, {
    expiresAt: Date.now() + PREVIEW_CACHE_TTL_MS,
    preview,
  })
  cachedCharacters += preview.content.length
  while (
    previewCache.size > PREVIEW_CACHE_MAX_ENTRIES ||
    cachedCharacters > PREVIEW_CACHE_MAX_CHARS
  ) {
    const oldestKey = previewCache.keys().next().value
    if (oldestKey === undefined) break
    deleteCachedPreview(oldestKey)
  }
}

async function fetchWorkspacePreview(
  rootPath: string,
  path: string
): Promise<CacheablePreview> {
  if (isImageFile(path)) {
    const base64 = await readWorkspaceFileBase64(rootPath, path)
    return {
      status: "image",
      path,
      content: toImageDataUrl(path, base64),
    }
  }
  const result = await readFilePreview(rootPath, path, TEXT_PREVIEW_MAX_BYTES)
  return {
    status: "text",
    path,
    content: result.content,
    truncated: result.truncated,
  }
}

export function loadWorkspacePreview(
  rootPath: string,
  path: string
): Promise<CacheablePreview> {
  const key = previewKey(rootPath, path)
  const cached = getCachedWorkspacePreview(rootPath, path)
  if (cached) return Promise.resolve(cached)
  const pending = previewRequests.get(key)
  if (pending) return pending

  const request = fetchWorkspacePreview(rootPath, path)
    .then((preview) => {
      cachePreview(key, preview)
      return preview
    })
    .finally(() => previewRequests.delete(key))
  previewRequests.set(key, request)
  return request
}
