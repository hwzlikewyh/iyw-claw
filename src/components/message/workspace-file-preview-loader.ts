import type { PreviewState } from "@/components/message/workspace-file-preview"
import { toImageDataUrl } from "@/components/message/workspace-file-preview"
import { readFilePreview, readWorkspaceFileBase64 } from "@/lib/api"
import { binaryPreviewKind } from "@/lib/binary-preview"
import { isImageFile, isOfficePreviewable } from "@/lib/language-detect"

const TEXT_PREVIEW_MAX_BYTES = 2 * 1024 * 1024
const HTML_PREVIEW_MAX_MEGABYTES = 20
const HTML_PREVIEW_LIMIT_BYTES = HTML_PREVIEW_MAX_MEGABYTES * 1024 * 1024

export type CacheablePreview = Extract<
  PreviewState,
  {
    status: "image" | "text" | "markdown" | "html" | "html-too-large"
  }
>

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

export function loadWorkspacePreview(
  rootPath: string,
  path: string,
  options?: { renderMarkdown?: boolean; renderHtml?: boolean }
): Promise<CacheablePreview> {
  return fetchWorkspacePreview(
    rootPath,
    path,
    options?.renderMarkdown === true,
    options?.renderHtml === true
  )
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
  const binary = binaryPreviewKind(path)
  if (binary) {
    return Promise.resolve({ status: "binary", path, kind: binary })
  }
  return loadWorkspacePreview(rootPath, path, options)
}

export function revokeWorkspacePreviewResource(state: PreviewState): void {
  if (state.status === "pdf" && state.src.startsWith("blob:")) {
    URL.revokeObjectURL(state.src)
  }
}
