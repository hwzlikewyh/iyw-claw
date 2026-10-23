import type { AssistantTimelineSection } from "./assistant-turn-timeline"
import { hasActiveProcess } from "./assistant-turn-timeline"
import { processPartHasFailure } from "./process-presentation"

const MIN_ARCHIVED_SECTIONS = 4
const VISIBLE_RECENT_SECTIONS = 0

interface ArchiveSectionItem {
  section: AssistantTimelineSection
  index: number
}

export type AssistantArchiveEntry =
  | { kind: "section"; key: string; item: ArchiveSectionItem }
  | { kind: "group"; key: string; items: ArchiveSectionItem[] }

function canArchive(section: AssistantTimelineSection): boolean {
  return (
    !section.retainOnCompletion &&
    !section.isFinalResponse &&
    section.parts.every((part) => part.type !== "reasoning") &&
    !section.parts.some(processPartHasFailure) &&
    !hasActiveProcess(section.parts)
  )
}

function appendRun(
  entries: AssistantArchiveEntry[],
  sections: AssistantTimelineSection[],
  start: number,
  end: number
) {
  const run = sections
    .slice(start, end)
    .map((section, offset) => ({ section, index: start + offset }))
  const archivedCount = run.length - VISIBLE_RECENT_SECTIONS
  if (archivedCount < MIN_ARCHIVED_SECTIONS) {
    for (const item of run) {
      entries.push({ kind: "section", key: item.section.key, item })
    }
    return
  }
  const items = run.slice(0, archivedCount)
  entries.push({
    kind: "group",
    key: "archive:" + items[0].section.key,
    items,
  })
  for (let offset = archivedCount; offset < run.length; offset += 1) {
    const item = run[offset]
    entries.push({ kind: "section", key: item.section.key, item })
  }
}

export function buildAssistantArchiveEntries(
  sections: AssistantTimelineSection[]
): AssistantArchiveEntry[] {
  const entries: AssistantArchiveEntry[] = []
  for (let index = 0; index < sections.length; ) {
    if (!canArchive(sections[index])) {
      const item = { section: sections[index], index }
      entries.push({ kind: "section", key: item.section.key, item })
      index += 1
      continue
    }
    const start = index
    while (index < sections.length && canArchive(sections[index])) index += 1
    appendRun(entries, sections, start, index)
  }
  return entries
}
