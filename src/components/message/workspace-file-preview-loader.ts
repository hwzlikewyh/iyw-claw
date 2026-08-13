import type { PreviewState } from "@/components/message/workspace-file-preview"
import { toImageDataUrl } from "@/components/message/workspace-file-preview"
import {
  readFileBase64,
  readFilePreview,
  readWorkspaceFileBase64,
} from "@/lib/api"
import { isImageFile } from "@/lib/language-detect"

const PREVIEW_CACHE_TTL_MS = 2_000
const PREVIEW_CACHE_MAX_ENTRIES = 8
const PREVIEW_CACHE_MAX_CHARS = 8_000_000
const TEXT_PREVIEW_MAX_BYTES = 2 * 1024 * 1024

export type CacheablePreview = Extract<
  PreviewState,
  { status: "image" | "text" | "markdown" }
>

const PDF_PREVIEW_MAX_BYTES = 30 * 1024 * 1024

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
  path: string,
  options?: { renderMarkdown?: boolean }
): CacheablePreview | null {
  const key = previewCacheKey(rootPath, path, options?.renderMarkdown === true)
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

function previewCacheKey(
  rootPath: string,
  path: string,
  renderMarkdown: boolean
): string {
  return `${previewKey(rootPath, path)}\0${renderMarkdown ? "markdown" : "text"}`
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
  path: string,
  renderMarkdown: boolean
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
    status: renderMarkdown && isMarkdownPath(path) ? "markdown" : "text",
    path,
    content: result.content,
    truncated: result.truncated,
  }
}

function isMarkdownPath(path: string): boolean {
  return /\.(?:md|markdown)$/i.test(path)
}

export async function loadPdfPreview(path: string): Promise<string> {
  const base64 = await readFileBase64(path, PDF_PREVIEW_MAX_BYTES)
  const binary = atob(base64)
  const bytes = Uint8Array.from(binary, (char) => char.charCodeAt(0))
  return URL.createObjectURL(new Blob([bytes], { type: "application/pdf" }))
}

export function loadWorkspacePreview(
  rootPath: string,
  path: string,
  options?: { renderMarkdown?: boolean }
): Promise<CacheablePreview> {
  const renderMarkdown = options?.renderMarkdown === true
  const key = previewCacheKey(rootPath, path, renderMarkdown)
  const cached = getCachedWorkspacePreview(rootPath, path, { renderMarkdown })
  if (cached) return Promise.resolve(cached)
  const pending = previewRequests.get(key)
  if (pending) return pending

  const request = fetchWorkspacePreview(rootPath, path, renderMarkdown)
    .then((preview) => {
      cachePreview(key, preview)
      return preview
    })
    .finally(() => previewRequests.delete(key))
  previewRequests.set(key, request)
  return request
}
