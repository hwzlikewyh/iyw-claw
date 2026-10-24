import type { AssistantTimelineSection } from "./assistant-turn-timeline"
import { hasActiveProcess } from "./assistant-turn-timeline"

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

function canArchive(
  section: AssistantTimelineSection,
  index: number,
  complete: boolean,
  total: number
): boolean {
  return (
    !section.retainOnCompletion &&
    !section.isFinalResponse &&
    !hasActiveProcess(section.parts) &&
    (complete || index < total - 1) &&
    section.parts.every(
      (part) => part.type !== "reasoning" || !part.isStreaming
    )
  )
}

export function buildAssistantArchiveEntries(
  sections: AssistantTimelineSection[],
  complete: boolean
): AssistantArchiveEntry[] {
  const entries: AssistantArchiveEntry[] = []
  for (let index = 0; index < sections.length; ) {
    const section = sections[index]
    if (section.retainOnCompletion || section.isFinalResponse) {
      const item = { section: sections[index], index }
      entries.push({ kind: "section", key: item.section.key, item })
      index += 1
      continue
    }

    const items: ArchiveSectionItem[] = []
    while (index < sections.length) {
      const current = sections[index]
      if (current.retainOnCompletion || current.isFinalResponse) break
      if (!canArchive(current, index, complete, sections.length)) {
        if (items.length > 0) {
          entries.push({
            kind: "group",
            key: "archive:" + items[0].section.key,
            items,
            settled: true,
          })
          items.length = 0
        }
        entries.push({
          kind: "section",
          key: current.key,
          item: { section: current, index },
        })
        index += 1
        continue
      }
      items.push({ section: current, index })
      index += 1
    }
    if (items.length > 0) {
      entries.push({
        kind: "group",
        key: "archive:" + items[0].section.key,
        items,
        settled: true,
      })
    }
  }
  return entries
}
