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

type ArchiveGroup = Extract<AssistantArchiveEntry, { kind: "group" }>

function continuesSegment(
  previous: AssistantTimelineSection,
  current: AssistantTimelineSection
): boolean {
  // 一段思考/说明及其后操作共用入口；压缩和下一轮思考/正文另起一段。
  if (previous.kind !== "content" || current.kind === "compaction") return false
  if (current.kind === "process") return true
  return (
    previous.parts.every((part) => part.type === "reasoning") &&
    current.parts.every((part) => part.type === "text")
  )
}

function segmentSettled(entry: ArchiveGroup, hasLaterContent: boolean) {
  return (
    hasLaterContent &&
    entry.items.every(
      ({ section }) =>
        !hasActiveProcess(section.parts) &&
        section.parts.every(
          (part) => part.type !== "reasoning" || !part.isStreaming
        )
    )
  )
}

export function buildAssistantArchiveEntries(
  sections: AssistantTimelineSection[],
  complete: boolean
): AssistantArchiveEntry[] {
  const entries: AssistantArchiveEntry[] = []
  for (let index = 0; index < sections.length; index += 1) {
    const section = sections[index]
    const item = { section, index }
    if (section.retainOnCompletion || section.isFinalResponse) {
      entries.push({ kind: "section", key: section.key, item })
      continue
    }
    const previous = entries[entries.length - 1]
    if (
      previous?.kind === "group" &&
      continuesSegment(
        previous.items[previous.items.length - 1].section,
        section
      )
    ) {
      previous.items.push(item)
    } else {
      // 每组独占数组；写入归档后不能清空或复用它，否则展开后步骤会丢失。
      entries.push({
        kind: "group",
        key: "archive:" + section.key,
        items: [item],
        settled: false,
      })
    }
  }
  for (const entry of entries) {
    if (entry.kind !== "group") continue
    const last = entry.items[entry.items.length - 1]
    entry.settled = segmentSettled(
      entry,
      complete || last.index < sections.length - 1
    )
  }
  return entries
}
