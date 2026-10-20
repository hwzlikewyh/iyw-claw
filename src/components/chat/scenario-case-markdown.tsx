"use client"

import { useEffect, useState } from "react"
import { Streamdown } from "streamdown"
import { useStreamdownPlugins } from "@/components/ai-elements/streamdown-plugins"
import { WorkspaceFilePreview } from "@/components/message/workspace-file-preview"
import type { ScenarioCaseAsset } from "@/lib/scenario-cases"
import { CaseLoadError } from "./scenario-case-error"

const MAX_CASE_TEXT_BYTES = 2 * 1024 * 1024

export function ScenarioCaseMarkdown({
  asset,
  onRetry,
}: {
  asset: ScenarioCaseAsset
  onRetry: () => void
}) {
  const { content, failed } = useCaseText(asset)
  const plugins = useStreamdownPlugins(
    asset.kind === "markdown" ? (content ?? "") : ""
  )
  if (failed) return <CaseLoadError onRetry={onRetry} />
  if (content === null)
    return (
      <p role="status" className="p-6 text-sm text-muted-foreground">
        正在加载文档…
      </p>
    )
  if (asset.kind === "text") return <CaseText asset={asset} content={content} />
  return (
    <article className="prose prose-sm max-w-none p-6 text-[#111827]">
      <Streamdown mode="static" plugins={plugins}>
        {content}
      </Streamdown>
    </article>
  )
}

function CaseText({
  asset,
  content,
}: {
  asset: ScenarioCaseAsset
  content: string
}) {
  const state = {
    path: asset.fileName,
    content,
    truncated: false,
  }
  return (
    <WorkspaceFilePreview
      rootPath=""
      state={
        asset.fileName.toLowerCase().endsWith(".csv")
          ? { ...state, status: "csv", delimiter: "," }
          : { ...state, status: "text" }
      }
    />
  )
}

function useCaseText(asset: ScenarioCaseAsset) {
  const [content, setContent] = useState<string | null>(null)
  const [failed, setFailed] = useState(false)
  useEffect(() => {
    const controller = new AbortController()
    void loadCaseText(asset.url, controller.signal)
      .then((value) => {
        if (!controller.signal.aborted) setContent(value)
      })
      .catch(() => {
        if (controller.signal.aborted) return
        console.warn("[scenario-case] text preview failed", {
          kind: asset.kind,
        })
        setFailed(true)
      })
    return () => controller.abort()
  }, [asset.url, asset.kind])
  return { content, failed }
}

async function loadCaseText(url: string, signal: AbortSignal) {
  const response = await fetch(url, { signal })
  if (!response.ok) throw new Error("Text preview unavailable")
  if (Number(response.headers.get("content-length")) > MAX_CASE_TEXT_BYTES) {
    throw new Error("Text preview exceeds size limit")
  }
  const bytes = await response.arrayBuffer()
  if (bytes.byteLength > MAX_CASE_TEXT_BYTES)
    throw new Error("Text preview exceeds size limit")
  return new TextDecoder().decode(bytes)
}
