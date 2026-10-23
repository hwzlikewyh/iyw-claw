import type { AssistantTimelineSection } from "./assistant-turn-timeline"
import { hasActiveProcess } from "./assistant-turn-timeline"
import { processPartHasFailure } from "./process-presentation"

interface ArchiveSectionItem {
  section: AssistantTimelineSection
  index: number
}

export type AssistantArchiveEntry =
  | { kind: "section"; key: string; item: ArchiveSectionItem }
  | {
      kind: "group"
      key: string
      items: ArchiveSectionItem[]
      settled: boolean
    }

function canArchive(section: AssistantTimelineSection): boolean {
  return (
    !section.retainOnCompletion &&
    !section.isFinalResponse &&
    section.parts.every((part) => part.type !== "reasoning") &&
    !section.parts.some(processPartHasFailure) &&
    !hasActiveProcess(section.parts)
  )
}

export function buildAssistantArchiveEntries(
  sections: AssistantTimelineSection[]
): AssistantArchiveEntry[] {
  const entries: AssistantArchiveEntry[] = []
  for (let index = 0; index < sections.length; ) {
    const section = sections[index]
    if (
      section.retainOnCompletion ||
      section.parts.some((part) => part.type === "reasoning")
    ) {
      const item = { section: sections[index], index }
      entries.push({ kind: "section", key: item.section.key, item })
      index += 1
      continue
    }
    const items = [{ section, index }]
    index += 1
    // 过程说明与其后的一段操作归档在一起；新段仍显示在原事件位置。
    if (section.kind === "content" && sections[index]?.kind === "process") {
      items.push({ section: sections[index], index })
      index += 1
    }
    entries.push({
      kind: "group",
      key: "archive:" + section.key,
      items,
      settled:
        index < sections.length &&
        items.some((item) => item.section.kind === "process") &&
        items.every((item) => canArchive(item.section)),
    })
  }
  return entries
}
