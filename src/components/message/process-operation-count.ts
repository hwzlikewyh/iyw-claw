import type { AdaptedContentPart } from "@/lib/adapters/ai-elements-adapter"

export function completedProcessOperations(
  parts: AdaptedContentPart[]
): number {
  const completed = new Set<string>()
  const collect = (items: AdaptedContentPart[]) => {
    for (const part of items) {
      if (part.type === "tool-call" || part.type === "tool-result") {
        if (
          part.state === "output-available" ||
          part.state === "output-error"
        ) {
          completed.add(part.toolCallId)
        }
      } else if (part.type === "tool-group") collect(part.items)
      else if (
        part.type === "delegation-status-group" ||
        part.type === "background-task-group"
      )
        collect(part.polls)
      else if (part.type === "goal-run") {
        collect([part.start, ...part.items, ...(part.end ? [part.end] : [])])
      }
    }
  }
  collect(parts)
  return completed.size
}
