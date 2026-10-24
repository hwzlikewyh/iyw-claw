import type { AdaptedContentPart } from "@/lib/adapters/ai-elements-adapter"
import { isContextCompactionMeta } from "@/lib/context-compaction"
import { normalizeToolName } from "@/lib/tool-call-normalization"
import { splitAssistantText } from "./assistant-text-segments"
import { getBuiltinToolDisplay } from "@/lib/builtin-tool-display"

export interface AssistantTimelineSection {
  key: string
  kind: "content" | "process" | "compaction"
  parts: AdaptedContentPart[]
  retainOnCompletion: boolean
  isFinalResponse: boolean
}

// 目标只包装生命周期，不改变正文、思考和工具的事件位置。
export function flattenAssistantParts(
  parts: AdaptedContentPart[]
): AdaptedContentPart[] {
  return parts.flatMap((part) => {
    if (part.type === "goal-run") {
      return [
        part.start,
        ...flattenAssistantParts(part.items),
        ...(part.end ? [part.end] : []),
      ]
    }
    if (part.type === "tool-group") return part.items
    if (
      part.type === "background-task-group" ||
      part.type === "delegation-status-group"
    )
      return part.polls
    return [part]
  })
}

function partKey(part: AdaptedContentPart, index: number): string {
  if (part.type === "tool-call" || part.type === "tool-result")
    return `${part.type}:${part.toolCallId}`
  if (part.type === "plugin-app") return `plugin:${part.instanceId}`
  return `${part.type}:${index}`
}

function isProcessPart(part: AdaptedContentPart) {
  if (part.type === "tool-call") {
    if (/(?:^|[./:_-])read_iyw_capability$/.test(part.toolName)) return true
    const name = normalizeToolName(
      getBuiltinToolDisplay(part.toolName, part.input)?.toolName ??
        part.toolName
    )
    return ![
      "attempt_completion",
      "show_interactive_html",
      "question",
    ].includes(name)
  }
  return (
    part.type === "tool-result" ||
    part.type === "delegation-status-group" ||
    part.type === "background-task-group"
  )
}

function retainResult(part: AdaptedContentPart): boolean {
  return (
    part.type === "displayed-image" ||
    part.type === "plugin-app" ||
    (part.type === "tool-call" && !isProcessPart(part))
  )
}

function finalTextStart(parts: AdaptedContentPart[]): number {
  let last = parts.length - 1
  while (last >= 0 && parts[last].type !== "text") last -= 1
  if (last < 0) return parts.length
  let start = last
  while (start > 0 && parts[start - 1].type === "text") start -= 1
  return start
}

function appendSection(
  sections: AssistantTimelineSection[],
  section: AssistantTimelineSection
) {
  const previous = sections[sections.length - 1]
  if (section.kind === "process" && previous?.kind === "process") {
    previous.parts.push(...section.parts)
  } else {
    sections.push(section)
  }
}

function appendFinalResponse(
  parts: AdaptedContentPart[],
  start: number,
  sections: AssistantTimelineSection[]
): number {
  const response: Extract<AdaptedContentPart, { type: "text" }>[] = []
  let index = start
  while (index < parts.length && parts[index].type === "text") {
    response.push(parts[index] as Extract<AdaptedContentPart, { type: "text" }>)
    index += 1
  }
  sections.push({
    key: "final-response:" + start,
    kind: "content",
    parts: response,
    retainOnCompletion: true,
    isFinalResponse: true,
  })
  return index
}

export function buildAssistantTimeline(
  parts: AdaptedContentPart[],
  complete: boolean
): AssistantTimelineSection[] {
  const ordered = flattenAssistantParts(parts).filter(
    (part) =>
      part.type !== "generated-image" &&
      (part.type !== "text" || Boolean(part.text.trim()))
  )
  const sections: AssistantTimelineSection[] = []
  const responseStart = complete ? finalTextStart(ordered) : ordered.length
  for (let index = 0; index < ordered.length; ) {
    if (index >= responseStart && ordered[index].type === "text") {
      index = appendFinalResponse(ordered, index, sections)
      continue
    }
    const source = ordered[index]
    const part =
      source.type === "reasoning"
        ? {
            ...source,
            isStreaming:
              source.isStreaming && !complete && index === ordered.length - 1,
          }
        : source
    const compaction =
      part.type === "tool-call" && isContextCompactionMeta(part.meta)
    const kind = compaction
      ? "compaction"
      : isProcessPart(part)
        ? "process"
        : "content"
    // 正在增长的末段交由 Streamdown 增量渲染，后续事件到达后再切分归档。
    const chunks =
      part.type === "text" && index < ordered.length - 1
        ? splitAssistantText(part.text).map((text) => ({ ...part, text }))
        : [part]
    for (const [chunk, value] of chunks.entries()) {
      appendSection(sections, {
        key: `${partKey(part, index)}:${chunk}`,
        kind,
        parts: [value],
        retainOnCompletion: compaction || retainResult(part),
        isFinalResponse: false,
      })
    }
    index += 1
  }
  return sections
}

export function hasActiveProcess(parts: AdaptedContentPart[]): boolean {
  return parts.some((part) => {
    if (part.type === "tool-call")
      return (
        part.state === "input-streaming" || part.state === "input-available"
      )
    if (
      part.type === "delegation-status-group" ||
      part.type === "background-task-group"
    )
      return hasActiveProcess(part.polls)
    return false
  })
}
