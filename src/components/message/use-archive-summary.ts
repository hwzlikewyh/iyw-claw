import { useMemo } from "react"
import { useTranslations } from "next-intl"
import type { AdaptedContentPart } from "@/lib/adapters/ai-elements-adapter"
import { getToolDisplayName } from "@/lib/tool-display"
import { isContextCompactionMeta } from "@/lib/context-compaction"
import { processToolTitle } from "./assistant-tool-presentation"
import { assistantResponseFirstLine } from "./assistant-text-segments"

const SUMMARY_TEXT_LIMIT = 480
const SUMMARY_LABEL_LIMIT = 52
const SUMMARY_ACTION_LIMIT = 24
const SUMMARY_ACTION_COUNT = 2

function shorten(text: string, limit: number) {
  const characters = Array.from(text.replace(/\s+/g, " ").trim())
  return characters.length > limit
    ? characters.slice(0, limit - 1).join("") + "…"
    : characters.join("")
}

function textSummary(parts: AdaptedContentPart[], reasoning = false) {
  for (const part of parts) {
    const text = reasoning
      ? part.type === "reasoning" && part.content
      : part.type === "text" && part.text
    if (!text || !text.trim()) continue
    // 摘要只解析有界片段，不为折叠标题重复解析整段长正文。
    const label = assistantResponseFirstLine([
      text.slice(0, SUMMARY_TEXT_LIMIT),
    ])
    if (label) return shorten(label, SUMMARY_LABEL_LIMIT)
  }
  return ""
}

function toolSummary(
  parts: AdaptedContentPart[],
  title: (tool: Extract<AdaptedContentPart, { type: "tool-call" }>) => string
) {
  const labels = new Set<string>()
  for (const part of parts) {
    if (part.type !== "tool-call") continue
    const label = title(part).replace(/\s+/g, " ").trim()
    if (label) labels.add(label)
    if (labels.size === SUMMARY_ACTION_COUNT) break
  }
  const limit = labels.size === 1 ? SUMMARY_LABEL_LIMIT : SUMMARY_ACTION_LIMIT
  return [...labels].map((label) => shorten(label, limit)).join(" · ")
}

export function useArchiveSummary(parts: AdaptedContentPart[], enabled = true) {
  const content = useTranslations("Folder.chat.contentParts")
  const activity = useTranslations("ConversationPresentation.toolActivity")
  return useMemo(() => {
    if (!enabled) return ""
    const label = textSummary(parts)
    if (label) return label
    const tools = toolSummary(parts, (tool) => {
      if (isContextCompactionMeta(tool.meta))
        return content("contextCompaction.compacted")
      const fallback = getToolDisplayName(tool, (key, values) =>
        content.has(key as never)
          ? content(key as never, values as never)
          : null
      )
      return processToolTitle(
        tool,
        (key, values) => activity(key as never, values as never),
        fallback
      )
    })
    return tools || textSummary(parts, true)
  }, [parts, enabled, content, activity])
}
