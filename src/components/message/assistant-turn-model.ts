import type { AdaptedContentPart } from "@/lib/adapters/ai-elements-adapter"

export interface AssistantTurnSections {
  processParts: AdaptedContentPart[]
  reasoningParts: Extract<AdaptedContentPart, { type: "reasoning" }>[]
  resultParts: AdaptedContentPart[]
  responseParts: Extract<AdaptedContentPart, { type: "text" }>[]
}

function isResultPart(part: AdaptedContentPart): boolean {
  return part.type === "generated-image" || part.type === "displayed-image"
}

export function splitAssistantTurnParts(
  parts: AdaptedContentPart[],
  complete: boolean
): AssistantTurnSections {
  const summaryIndex = complete
    ? parts.reduce(
        (last, part, index) =>
          part.type === "text" && part.text.trim() ? index : last,
        -1
      )
    : -1

  // Keep body text and tool events in source order. Once a turn completes,
  // only the last non-empty text block is promoted to the visible summary;
  // everything else remains available in the collapsible execution stream.
  const processParts = parts.filter((part, index) => {
    if (isResultPart(part) || part.type === "reasoning") return false
    if (part.type === "text") {
      return Boolean(part.text.trim()) && index !== summaryIndex
    }
    return true
  })
  const responseParts = parts.filter(
    (part, index): part is Extract<AdaptedContentPart, { type: "text" }> =>
      part.type === "text" &&
      Boolean(part.text.trim()) &&
      complete &&
      index === summaryIndex
  )

  return {
    processParts,
    reasoningParts: parts.filter(
      (part): part is Extract<AdaptedContentPart, { type: "reasoning" }> =>
        part.type === "reasoning"
    ),
    resultParts: parts.filter(isResultPart),
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
