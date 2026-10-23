import type { AdaptedContentPart } from "@/lib/adapters/ai-elements-adapter"

export interface AssistantTimelineSection {
  key: string
  kind: "content" | "process"
  parts: AdaptedContentPart[]
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
    return part.type === "tool-group" ? part.items : [part]
  })
}

function partKey(part: AdaptedContentPart, index: number): string {
  if (part.type === "tool-call" || part.type === "tool-result")
    return `${part.type}:${part.toolCallId}`
  if (part.type === "plugin-app") return `plugin:${part.instanceId}`
  return `${part.type}:${index}`
}

function isProcessPart(part: AdaptedContentPart) {
  return (
    part.type === "reasoning" ||
    part.type === "tool-call" ||
    part.type === "tool-result" ||
    part.type === "delegation-status-group" ||
    part.type === "background-task-group"
  )
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
  ordered.forEach((source, index) => {
    const part =
      source.type === "reasoning"
        ? {
            ...source,
            isStreaming:
              source.isStreaming && !complete && index === ordered.length - 1,
          }
        : source
    const kind = isProcessPart(part) ? "process" : "content"
    const previous = sections[sections.length - 1]
    if (kind === "process" && previous?.kind === kind) {
      previous.parts.push(part)
    } else {
      sections.push({ key: partKey(part, index), kind, parts: [part] })
    }
  })
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
