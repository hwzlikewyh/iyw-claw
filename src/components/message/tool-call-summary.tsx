"use client"

import { WrenchIcon } from "lucide-react"
import { useTranslations } from "next-intl"
import type { AdaptedContentPart } from "@/lib/adapters/ai-elements-adapter"
import { getBuiltinToolDisplay } from "@/lib/builtin-tool-display"
import {
  getReadableToolActivityTitle,
  getToolDisplayName,
} from "@/lib/tool-display"
import { getStatusBadge } from "@/components/ai-elements/tool"

type ToolRecord = Extract<
  AdaptedContentPart,
  { type: "tool-call" | "tool-result" }
>

export function ToolCallSummary({ part }: { part: ToolRecord }) {
  const t = useTranslations("Folder.chat.contentParts")
  const status = useTranslations("Folder.chat.tool.status")
  const name = toolName(part)
  const title =
    name.displayTitle ??
    getToolDisplayName(name, (key, values) =>
      t.has(key as never) ? t(key as never, values as never) : null
    )
  const label = status(
    part.state === "input-streaming"
      ? "inputStreaming"
      : part.state === "input-available"
        ? "inputAvailable"
        : part.state === "output-error"
          ? "outputError"
          : "outputAvailable"
  )
  return (
    <div className="tool-call-summary flex min-w-0 items-center gap-2 rounded-md border border-border/60 px-3 py-2">
      <WrenchIcon
        aria-hidden="true"
        className="size-3.5 shrink-0 text-muted-foreground"
      />
      <span className="min-w-0 truncate text-sm font-medium">{title}</span>
      <span className="shrink-0">{getStatusBadge(part.state, label)}</span>
    </div>
  )
}

function toolName(part: ToolRecord) {
  if (part.type === "tool-result") return { toolName: "tool" }
  const builtin = getBuiltinToolDisplay(
    part.toolName,
    part.input,
    part.displayTitle
  )
  const builtinTitle = getReadableToolActivityTitle(builtin?.fallbackName)
  // 仅解析工具身份；描述、命令及其他参数不能成为前端标题。
  return {
    toolName: builtin?.toolName ?? part.toolName,
    displayTitle:
      builtinTitle ?? getReadableToolActivityTitle(part.displayTitle),
  }
}
