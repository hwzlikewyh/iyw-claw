import type { PreviewState } from "@/components/message/workspace-file-preview"
import { toImageDataUrl } from "@/components/message/workspace-file-preview"
import {
  readFileBase64,
  readFilePreview,
  readWorkspaceFileBase64,
} from "@/lib/api"
import { joinRootRel } from "@/lib/file-open-target"
import { isImageFile, isOfficePreviewable } from "@/lib/language-detect"

const PREVIEW_CACHE_TTL_MS = 2_000
const PREVIEW_CACHE_MAX_ENTRIES = 8
const PREVIEW_CACHE_MAX_CHARS = 8_000_000
const TEXT_PREVIEW_MAX_BYTES = 2 * 1024 * 1024
const HTML_PREVIEW_MAX_MEGABYTES = 20
const HTML_PREVIEW_LIMIT_BYTES = HTML_PREVIEW_MAX_MEGABYTES * 1024 * 1024

export type CacheablePreview = Extract<
  PreviewState,
  {
    status: "image" | "text" | "markdown" | "html" | "html-too-large"
  }
>

const PDF_PREVIEW_MAX_BYTES = 30 * 1024 * 1024

interface CachedPreview {
  expiresAt: number
  preview: CacheablePreview
}

const previewCache = new Map<string, CachedPreview>()
const previewRequests = new Map<string, Promise<CacheablePreview>>()
let cachedCharacters = 0

function previewCharacters(preview: CacheablePreview): number {
  return "content" in preview ? preview.content.length : 0
}

function previewKey(rootPath: string, path: string): string {
  return `${rootPath}\0${path}`
}

function deleteCachedPreview(key: string): void {
  const cached = previewCache.get(key)
  if (!cached) return
  cachedCharacters -= previewCharacters(cached.preview)
  previewCache.delete(key)
}

export function getCachedWorkspacePreview(
  rootPath: string,
  path: string,
  options?: { renderMarkdown?: boolean; renderHtml?: boolean }
): CacheablePreview | null {
  const key = previewCacheKey(
    rootPath,
    path,
    options?.renderMarkdown === true,
    options?.renderHtml === true
  )
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
  renderMarkdown: boolean,
  renderHtml: boolean
): string {
  const mode = `${renderMarkdown ? "markdown" : ""}:${renderHtml ? "html" : ""}`
  return `${previewKey(rootPath, path)}\0${mode}`
}

function cachePreview(key: string, preview: CacheablePreview): void {
  deleteCachedPreview(key)
  const characters = previewCharacters(preview)
  if (characters > PREVIEW_CACHE_MAX_CHARS / 2) return
  previewCache.set(key, {
    expiresAt: Date.now() + PREVIEW_CACHE_TTL_MS,
    preview,
  })
  cachedCharacters += characters
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
  renderMarkdown: boolean,
  renderHtml: boolean
): Promise<CacheablePreview> {
  if (isImageFile(path)) {
    const base64 = await readWorkspaceFileBase64(rootPath, path)
    return {
      status: "image",
      path,
      content: toImageDataUrl(path, base64),
    }
  }
  const html = renderHtml && isHtmlPath(path)
  const result = await readFilePreview(
    rootPath,
    path,
    html ? HTML_PREVIEW_LIMIT_BYTES - 1 : TEXT_PREVIEW_MAX_BYTES
  )
  if (html && result.truncated) {
    return {
      status: "html-too-large",
      path,
      maxMegabytes: HTML_PREVIEW_MAX_MEGABYTES,
    }
  }
  const status =
    renderMarkdown && isMarkdownPath(path) ? "markdown" : html ? "html" : "text"
  return {
    status,
    path,
    content: result.content,
    truncated: result.truncated,
  }
}

function isMarkdownPath(path: string): boolean {
  return /\.(?:md|markdown)$/i.test(path)
}

function isHtmlPath(path: string): boolean {
  return /\.(?:html|htm)$/i.test(path)
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
  options?: { renderMarkdown?: boolean; renderHtml?: boolean }
): Promise<CacheablePreview> {
  const renderMarkdown = options?.renderMarkdown === true
  const renderHtml = options?.renderHtml === true
  const key = previewCacheKey(rootPath, path, renderMarkdown, renderHtml)
  const cached = getCachedWorkspacePreview(rootPath, path, {
    renderMarkdown,
    renderHtml,
  })
  if (cached) return Promise.resolve(cached)
  const pending = previewRequests.get(key)
  if (pending) return pending

  const request = fetchWorkspacePreview(
    rootPath,
    path,
    renderMarkdown,
    renderHtml
  )
    .then((preview) => {
      cachePreview(key, preview)
      return preview
    })
    .finally(() => previewRequests.delete(key))
  previewRequests.set(key, request)
  return request
}

export function loadWorkspaceFilePreview(
  rootPath: string,
  path: string,
  options?: {
    renderMarkdown?: boolean
    renderHtml?: boolean
    renderPdf?: boolean
  }
): Promise<PreviewState> {
  if (isOfficePreviewable(path)) {
    return Promise.resolve({ status: "office", path })
  }
  if (options?.renderPdf === true && isPdfPath(path)) {
    return loadPdfPreview(joinRootRel(rootPath, path)).then((src) => ({
      status: "pdf",
      path,
      src,
    }))
  }
  return loadWorkspacePreview(rootPath, path, options)
}

export function revokeWorkspacePreviewResource(state: PreviewState): void {
  if (state.status === "pdf" && state.src.startsWith("blob:")) {
    URL.revokeObjectURL(state.src)
  }
}

function isPdfPath(path: string): boolean {
  return /\.pdf$/i.test(path)
}
