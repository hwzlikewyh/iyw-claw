import type { AdaptedContentPart } from "@/lib/adapters/ai-elements-adapter"

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
  const summaryIndex = parts.reduce(
    (last, part, index) =>
      part.type === "text" && part.text.trim() ? index : last,
    -1
  )

  // 流式和完成态共用正文位置；思考与工具事件保持原顺序收纳到过程。
  const processParts = parts.filter((part, index) => {
    if (isFinalResultPart(part)) return false
    if (part.type === "text") {
      return Boolean(part.text.trim()) && index !== summaryIndex
    }
    return true
  })
  const responseParts = parts.filter(
    (part, index): part is Extract<AdaptedContentPart, { type: "text" }> =>
      part.type === "text" &&
      Boolean(part.text.trim()) &&
      index === summaryIndex
  )

  return {
    processParts,
    resultParts: parts.filter(isFinalResultPart),
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
