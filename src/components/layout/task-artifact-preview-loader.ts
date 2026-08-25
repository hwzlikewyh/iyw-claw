import type { TaskArtifactTarget } from "@/components/layout/task-artifact-actions"
import type { PreviewState } from "@/components/message/workspace-file-preview"
import {
  loadPdfPreview,
  loadWorkspacePreview,
} from "@/components/message/workspace-file-preview-loader"
import type { TaskArtifactInfo } from "@/lib/api"
import { joinRootRel } from "@/lib/file-open-target"
import { isOfficePreviewable } from "@/lib/language-detect"

export interface LoadedArtifactPreview {
  key: string
  state: PreviewState
}

export interface ArtifactPreviewSource {
  key: string
  kind: TaskArtifactInfo["kind"]
  path: string
  status: TaskArtifactInfo["status"]
}

export function startArtifactPreviewLoad(
  artifact: ArtifactPreviewSource,
  target: TaskArtifactTarget | null,
  failureMessage: string,
  setLoaded: (loaded: LoadedArtifactPreview) => void
): (() => void) | undefined {
  if (artifact.status !== "available") return
  const controller = new AbortController()
  const request = createArtifactPreviewRequest(
    artifact,
    target,
    controller.signal
  )
  if (!request) return
  const key = artifact.key
  let active = true
  void request
    .then((state) => active && setLoaded({ key, state }))
    .catch(() => {
      if (active) {
        setLoaded({
          key,
          state: {
            status: "error",
            path: artifact.path,
            message: failureMessage,
          },
        })
      }
    })
  return () => {
    active = false
    controller.abort()
    void request.then(revokePreviewResource).catch(() => undefined)
  }
}

function createArtifactPreviewRequest(
  artifact: ArtifactPreviewSource,
  target: TaskArtifactTarget | null,
  signal: AbortSignal
): Promise<PreviewState> | null {
  if (artifact.kind === "url") {
    return isRemoteMarkdown(artifact.path)
      ? loadRemoteMarkdownPreview(artifact.path, signal)
      : null
  }
  if (!target || isOfficePreviewable(target.ioPath)) return null
  if (isPdfPath(target.ioPath)) {
    return loadPdfPreview(joinRootRel(target.rootPath, target.ioPath)).then(
      (src) => ({ status: "pdf", path: target.ioPath, src })
    )
  }
  return loadWorkspacePreview(target.rootPath, target.ioPath, {
    renderMarkdown: true,
    renderHtml: true,
  })
}

export function resolveArtifactPreview(
  artifact: TaskArtifactInfo,
  target: TaskArtifactTarget | null,
  loaded: LoadedArtifactPreview | null,
  messages: { unavailable: string; failed: string }
): PreviewState {
  if (artifact.status !== "available") {
    return {
      status: "error",
      path: artifact.path,
      message: messages.unavailable,
    }
  }
  if (artifact.kind === "url") {
    return resolveRemoteArtifactPreview(artifact, loaded)
  }
  if (!target) {
    return { status: "error", path: artifact.path, message: messages.failed }
  }
  if (isOfficePreviewable(target.ioPath)) {
    return { status: "office", path: target.ioPath }
  }
  if (loaded?.key === artifactPreviewKey(artifact)) return loaded.state
  return { status: "loading", path: target.ioPath }
}

function resolveRemoteArtifactPreview(
  artifact: TaskArtifactInfo,
  loaded: LoadedArtifactPreview | null
): PreviewState {
  if (loaded?.key === artifactPreviewKey(artifact)) return loaded.state
  if (isRemoteMarkdown(artifact.path)) {
    return { status: "loading", path: artifact.path }
  }
  if (urlPathMatches(artifact.path, /\.pdf$/i)) {
    return { status: "pdf", path: artifact.path, src: artifact.path }
  }
  return {
    status: "url",
    path: artifact.path,
    src: remoteArtifactFrameSrc(artifact.path),
  }
}

function artifactPreviewKey(artifact: TaskArtifactInfo): string {
  return `${artifact.id}:${artifact.lastCheckedAt}`
}

function revokePreviewResource(state: PreviewState): void {
  if (state.status === "pdf" && state.src.startsWith("blob:")) {
    URL.revokeObjectURL(state.src)
  }
}

function isPdfPath(path: string): boolean {
  return /\.pdf$/i.test(path)
}

function remoteArtifactFrameSrc(path: string): string {
  if (urlPathMatches(path, /\.(?:doc|docx|xls|xlsx|ppt|pptx)$/i)) {
    return `https://view.officeapps.live.com/op/embed.aspx?src=${encodeURIComponent(path)}`
  }
  return path
}

function isRemoteMarkdown(path: string): boolean {
  return urlPathMatches(path, /\.(?:md|markdown)$/i)
}

async function loadRemoteMarkdownPreview(
  path: string,
  signal: AbortSignal
): Promise<PreviewState> {
  try {
    const controller = new AbortController()
    const abort = () => controller.abort()
    signal.addEventListener("abort", abort, { once: true })
    const timer = window.setTimeout(() => controller.abort(), 15_000)
    try {
      const response = await fetch(path, {
        signal: controller.signal,
        credentials: "omit",
      })
      if (!response.ok) throw new Error(`HTTP ${response.status}`)
      const content = await readBoundedResponseText(response, 4 * 1024 * 1024)
      return { status: "markdown", path, content, truncated: false }
    } finally {
      window.clearTimeout(timer)
      signal.removeEventListener("abort", abort)
    }
  } catch {
    if (signal.aborted) throw new DOMException("Aborted", "AbortError")
    return { status: "url", path, src: remoteArtifactFrameSrc(path) }
  }
}

async function readBoundedResponseText(
  response: Response,
  maxBytes: number
): Promise<string> {
  const reader = response.body?.getReader()
  if (!reader) {
    const text = await response.text()
    if (new TextEncoder().encode(text).byteLength > maxBytes) {
      throw new Error("markdown too large")
    }
    return text
  }
  const decoder = new TextDecoder()
  let total = 0
  let text = ""
  while (true) {
    const { done, value } = await reader.read()
    if (done) break
    total += value.byteLength
    if (total > maxBytes) {
      await reader.cancel()
      throw new Error("markdown too large")
    }
    text += decoder.decode(value, { stream: true })
  }
  return text + decoder.decode()
}

function urlPathMatches(value: string, pattern: RegExp): boolean {
  try {
    return pattern.test(new URL(value).pathname)
  } catch {
    return false
  }
}
