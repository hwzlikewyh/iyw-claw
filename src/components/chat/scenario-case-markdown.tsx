"use client"

import { useEffect, useState } from "react"
import { Streamdown } from "streamdown"
import { useStreamdownPlugins } from "@/components/ai-elements/streamdown-plugins"
import type { ScenarioCaseAsset } from "@/lib/scenario-cases"
import { CaseLoadError } from "./scenario-case-error"

const MAX_MARKDOWN_BYTES = 2 * 1024 * 1024

export function ScenarioCaseMarkdown({
  asset,
  onRetry,
}: {
  asset: ScenarioCaseAsset
  onRetry: () => void
}) {
  const [content, setContent] = useState<string | null>(null)
  const [failed, setFailed] = useState(false)
  const plugins = useStreamdownPlugins(content ?? "")
  useEffect(() => {
    const controller = new AbortController()
    void loadCaseMarkdown(asset.url, controller.signal)
      .then((value) => {
        if (!controller.signal.aborted) setContent(value)
      })
      .catch(() => {
        if (controller.signal.aborted) return
        console.warn("[scenario-case] markdown preview failed", {
          kind: asset.kind,
        })
        setFailed(true)
      })
    return () => controller.abort()
  }, [asset.url, asset.kind])
  if (failed) return <CaseLoadError onRetry={onRetry} />
  if (content === null)
    return (
      <p role="status" className="p-6 text-sm text-muted-foreground">
        正在加载文档…
      </p>
    )
  return (
    <article className="prose prose-sm max-w-none p-6 text-[#111827]">
      <Streamdown mode="static" plugins={plugins}>
        {content}
      </Streamdown>
    </article>
  )
}

async function loadCaseMarkdown(url: string, signal: AbortSignal) {
  const response = await fetch(url, { signal })
  if (!response.ok) throw new Error("Markdown preview unavailable")
  if (Number(response.headers.get("content-length")) > MAX_MARKDOWN_BYTES) {
    throw new Error("Markdown preview exceeds size limit")
  }
  const bytes = await response.arrayBuffer()
  if (bytes.byteLength > MAX_MARKDOWN_BYTES)
    throw new Error("Markdown preview exceeds size limit")
  return new TextDecoder().decode(bytes)
}
