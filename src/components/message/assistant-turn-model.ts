import type { AdaptedContentPart } from "@/lib/adapters/ai-elements-adapter"
import { flattenAssistantParts } from "./assistant-turn-timeline"

export interface AssistantTurnSections {
  processParts: AdaptedContentPart[]
  resultParts: AdaptedContentPart[]
  responseParts: Extract<AdaptedContentPart, { type: "text" }>[]
}

function isFinalResultPart(part: AdaptedContentPart): boolean {
  return part.type === "displayed-image"
}

export function splitAssistantTurnParts(
  parts: AdaptedContentPart[],
  // eslint-disable-next-line @typescript-eslint/no-unused-vars -- 保留既有调用签名，流式和完成态共用分区。
  _complete: boolean
): AssistantTurnSections {
  const ordered = flattenAssistantParts(parts)
  const processParts = ordered.filter((part) => {
    if (isFinalResultPart(part)) return false
    return part.type !== "text"
  })
  const responseParts = ordered.filter(
    (part): part is Extract<AdaptedContentPart, { type: "text" }> =>
      part.type === "text" && Boolean(part.text.trim())
  )

  return {
    processParts,
    resultParts: ordered.filter(isFinalResultPart),
    responseParts,
  }
}

export function processPartHasError(part: AdaptedContentPart): boolean {
  if (part.type === "tool-call" || part.type === "tool-result") {
    return part.state === "output-error" || Boolean(part.errorText?.trim())
  }
  if (part.type === "tool-group") return part.items.some(processPartHasError)
  if (part.type === "delegation-status-group") {
    return part.polls.some(processPartHasError)
  }
  if (part.type === "background-task-group") {
    return part.polls.some(processPartHasError)
  }
  if (part.type === "goal-run") {
    return (
      processPartHasError(part.start) ||
      Boolean(part.end && processPartHasError(part.end)) ||
      part.items.some(processPartHasError)
    )
  }
  return false
}

export function countProcessItems(parts: AdaptedContentPart[]): number {
  return parts.reduce((count, part) => {
    if (part.type === "tool-group") return count + part.items.length
    if (part.type === "delegation-status-group")
      return count + part.polls.length
    if (part.type === "background-task-group") return count + part.polls.length
    if (part.type === "goal-run") {
      return count + 1 + countProcessItems(part.items) + (part.end ? 1 : 0)
    }
    if (part.type === "text" && !part.text.trim()) return count
    return count + 1
  }, 0)
}
