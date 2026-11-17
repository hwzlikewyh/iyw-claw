import { create } from "zustand"
import type { KnowledgeSelection } from "./cloud-knowledge"

interface KnowledgeComposerEntry {
  selection?: KnowledgeSelection
}

export const useKnowledgeComposerContext = create<{
  entries: Record<string, KnowledgeComposerEntry>
}>(() => ({ entries: {} }))

export function setKnowledgeComposerSelection(
  tabId: string,
  selection: KnowledgeSelection
) {
  useKnowledgeComposerContext.setState(({ entries }) => {
    const current = entries[tabId]?.selection
    if (
      current &&
      current.scope === selection.scope &&
      current.revision > selection.revision
    )
      return { entries }
    return { entries: { ...entries, [tabId]: { selection } } }
  })
}

export function clearKnowledgeComposerSelection(tabId: string) {
  useKnowledgeComposerContext.setState(({ entries }) => {
    const next = { ...entries }
    delete next[tabId]
    return { entries: next }
  })
}
