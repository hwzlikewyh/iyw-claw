import type { AdaptedContentPart } from "@/lib/adapters/ai-elements-adapter"

export interface AssistantTurnSections {
  processParts: AdaptedContentPart[]
  resultParts: AdaptedContentPart[]
  summaryParts: AdaptedContentPart[]
}

function isResultPart(part: AdaptedContentPart): boolean {
  return part.type === "generated-image" || part.type === "displayed-image"
}

function findSummaryIndex(parts: AdaptedContentPart[]): number {
  for (let index = parts.length - 1; index >= 0; index -= 1) {
    const part = parts[index]
    if (part.type === "text" && part.text.trim()) return index
  }
  return -1
}

export function splitAssistantTurnParts(
  parts: AdaptedContentPart[],
  complete: boolean
): AssistantTurnSections {
  const summaryIndex = complete ? findSummaryIndex(parts) : -1
  return {
    processParts: parts.filter(
      (part, index) => index !== summaryIndex && !isResultPart(part)
    ),
    resultParts: parts.filter(isResultPart),
    summaryParts: summaryIndex >= 0 ? [parts[summaryIndex]] : [],
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
