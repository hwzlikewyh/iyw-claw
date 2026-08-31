"use client"

import { AlertCircle, FileCode2, Loader2 } from "lucide-react"
import { useTranslations } from "next-intl"
import { useState } from "react"
import { Streamdown } from "streamdown"

import { useStreamdownPlugins } from "@/components/ai-elements/streamdown-plugins"
import { HtmlPreview } from "@/components/files/html-preview"
import { OfficePreview } from "@/components/files/office-preview"
import { EditableImagePreview } from "@/components/ui/editable-image-preview"
import { useWorkspaceMarkdownOptions } from "@/components/message/markdown-workspace-links"
import { useAppWorkspaceStore } from "@/stores/app-workspace-store"
import { joinRootRel } from "@/lib/file-open-target"

export type PreviewState =
  | { status: "idle" }
  | { status: "loading"; path: string }
  | { status: "text"; path: string; content: string; truncated: boolean }
  | { status: "image"; path: string; content: string }
  | { status: "office"; path: string }
  | { status: "markdown"; path: string; content: string; truncated: boolean }
  | { status: "html"; path: string; content: string; truncated: boolean }
  | { status: "html-too-large"; path: string; maxMegabytes: number }
  | { status: "pdf"; path: string; src: string }
  | { status: "url"; path: string; src: string }
  | {
      status: "media"
      mediaType: RemoteArtifactMediaType
      path: string
      src: string
    }
  | { status: "error"; path: string; message: string }

export type RemoteArtifactMediaType = "image" | "video" | "audio"

const IMAGE_MIME_TYPES: Record<string, string> = {
  bmp: "image/bmp",
  gif: "image/gif",
  ico: "image/x-icon",
  jpeg: "image/jpeg",
  jpg: "image/jpeg",
  png: "image/png",
  svg: "image/svg+xml",
  webp: "image/webp",
}

export function toImageDataUrl(path: string, base64: string): string {
  const extension = path.split(".").pop()?.toLowerCase() ?? ""
  const mimeType = IMAGE_MIME_TYPES[extension] ?? "application/octet-stream"
  return `data:${mimeType};base64,${base64}`
}

function fileName(path: string): string {
  return path.split(/[/\\]/).pop() || path
}

function PreviewStatus({ state }: { state: PreviewState }) {
  const t = useTranslations("Folder.chat.workspaceFiles")
  if (state.status === "idle") {
    return (
      <div className="flex h-full flex-col items-center justify-center gap-3 px-6 text-center text-muted-foreground">
        <FileCode2 className="size-8 opacity-60" />
        <p className="text-sm">{t("selectFile")}</p>
      </div>
    )
  }
  if (state.status === "loading") {
    return (
      <div className="flex h-full items-center justify-center gap-2 text-sm text-muted-foreground">
        <Loader2 className="size-4 animate-spin" />
        {t("loadingPreview")}
      </div>
    )
  }
  if (state.status === "html-too-large") {
    return (
      <div className="flex h-full flex-col items-center justify-center gap-3 px-6 text-center">
        <AlertCircle className="size-8 text-muted-foreground/80" />
        <p className="text-sm font-medium">{t("previewError")}</p>
        <p className="max-w-lg text-xs text-muted-foreground">
          {t("htmlPreviewTooLarge", { size: state.maxMegabytes })}
        </p>
      </div>
    )
  }
  if (state.status !== "error") return null
  return (
    <div className="flex h-full flex-col items-center justify-center gap-3 px-6 text-center">
      <AlertCircle className="size-8 text-destructive/80" />
      <p className="text-sm font-medium">{t("previewError")}</p>
      <p className="max-w-lg break-words text-xs text-muted-foreground">
        {state.message}
      </p>
    </div>
  )
}

export function WorkspaceFilePreview({
  state,
  rootPath,
  htmlRootPath,
  onOpenWorkspace,
}: {
  state: PreviewState
  rootPath: string
  htmlRootPath?: string
  onOpenWorkspace?: () => void
}) {
  if (
    state.status === "idle" ||
    state.status === "loading" ||
    state.status === "html-too-large" ||
    state.status === "error"
  ) {
    return <PreviewStatus state={state} />
  }
  if (state.status === "image") {
    return (
      <div className="flex h-full items-center justify-center overflow-auto bg-muted/20 p-6">
        <EditableImagePreview
          src={state.content}
          alt={fileName(state.path)}
          trigger="edit-button"
          className="flex size-full items-center justify-center"
          imageProps={{
            className: "max-h-full max-w-full object-contain",
          }}
        />
      </div>
    )
  }
  if (state.status === "office") {
    return <OfficePreview rootPath={rootPath} relPath={state.path} />
  }
  if (state.status === "pdf") {
    const remotePdf = !state.src.startsWith("blob:")
    return (
      <iframe
        title={fileName(state.path)}
        src={state.src}
        sandbox={
          remotePdf ? "allow-scripts allow-forms allow-popups" : undefined
        }
        referrerPolicy="no-referrer"
        className="h-full w-full border-0 bg-white"
      />
    )
  }
  if (state.status === "url") {
    return <RemoteUrlPreview path={state.path} src={state.src} />
  }
  if (state.status === "media") {
    return (
      <RemoteMediaPreview
        key={`${state.mediaType}:${state.src}`}
        state={state}
      />
    )
  }
  if (state.status === "html") {
    const absolutePath = joinRootRel(rootPath, state.path)
    return (
      <HtmlPreview
        key={absolutePath}
        content={state.content}
        path={absolutePath}
        rootPath={htmlRootPath ?? rootPath}
      />
    )
  }
  if (state.status === "markdown") {
    return (
      <MarkdownPreview
        state={state}
        rootPath={rootPath}
        onOpenWorkspace={onOpenWorkspace}
      />
    )
  }
  return <TextPreview state={state} />
}

function RemoteMediaPreview({
  state,
}: {
  state: Extract<PreviewState, { status: "media" }>
}) {
  const [failed, setFailed] = useState(false)
  if (failed) return <RemoteUrlPreview path={state.path} src={state.src} />

  if (state.mediaType !== "image") {
    return <RemoteUrlPreview path={state.path} src={state.src} />
  }

  return (
    <div className="flex h-full min-h-0 w-full items-center justify-center overflow-auto bg-muted/20 p-6">
      {/* eslint-disable-next-line @next/next/no-img-element */}
      <img
        src={state.src}
        referrerPolicy="no-referrer"
        onError={() => setFailed(true)}
        alt={fileName(state.path)}
        className="max-h-full max-w-full object-contain"
      />
    </div>
  )
}

function RemoteUrlPreview({ path, src }: { path: string; src: string }) {
  const officeOnline = src.startsWith("https://view.officeapps.live.com/")
  return (
    <iframe
      title={fileName(path)}
      src={src}
      sandbox={
        officeOnline
          ? "allow-scripts allow-same-origin allow-forms allow-popups"
          : "allow-scripts allow-forms allow-popups"
      }
      referrerPolicy="no-referrer"
      className="h-full w-full border-0 bg-white"
    />
  )
}

function MarkdownPreview({
  state,
  rootPath,
  onOpenWorkspace,
}: {
  state: Extract<PreviewState, { status: "markdown" }>
  rootPath: string
  onOpenWorkspace?: () => void
}) {
  const plugins = useStreamdownPlugins(state.content)
  const roots = useAppWorkspaceStore((store) => store.folders)
  const options = useWorkspaceMarkdownOptions({
    content: state.content,
    documentPath: rootPath ? joinRootRel(rootPath, state.path) : "",
    roots,
    onOpenWorkspace,
  })
  return (
    <div className="h-full overflow-auto p-6 [&_ol]:list-decimal [&_ol]:pl-6 [&_ul]:list-disc [&_ul]:pl-6">
      <Streamdown plugins={plugins} {...options}>
        {state.content}
      </Streamdown>
    </div>
  )
}

function TextPreview({
  state,
}: {
  state: Extract<PreviewState, { status: "text" }>
}) {
  const t = useTranslations("Folder.chat.workspaceFiles")
  return (
    <div className="grid h-full min-h-0 grid-rows-[minmax(0,1fr)_auto]">
      <pre className="overflow-auto whitespace-pre p-4 font-mono text-xs leading-5 text-foreground">
        {state.content}
      </pre>
      {state.truncated && (
        <div className="border-t bg-muted/20 px-4 py-2 text-xs text-muted-foreground">
          {t("previewTruncated")}
        </div>
      )}
    </div>
  )
}
