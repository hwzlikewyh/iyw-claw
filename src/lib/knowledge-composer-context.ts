import { create } from "zustand"
import type { KnowledgeSelection } from "./cloud-knowledge"

interface KnowledgeComposerEntry {
  selection?: KnowledgeSelection
  accountScope?: string
  version: number
}

export const useKnowledgeComposerContext = create<{
  entries: Record<string, KnowledgeComposerEntry>
}>(() => ({ entries: {} }))

export function setKnowledgeComposerSelection(
  tabId: string,
  selection: KnowledgeSelection
) {
  useKnowledgeComposerContext.setState(({ entries }) => {
    const current = entries[tabId]
    if (current?.accountScope && current.accountScope !== selection.scope)
      return { entries }
    return {
      entries: {
        ...entries,
        [tabId]: {
          selection,
          accountScope: selection.scope,
          version: (current?.version ?? 0) + 1,
        },
      },
    }
  })
}

export function clearKnowledgeComposerSelection(tabId: string) {
  useKnowledgeComposerContext.setState(({ entries }) => {
    return {
      entries: {
        ...entries,
        [tabId]: {
          accountScope: entries[tabId]?.accountScope,
          version: (entries[tabId]?.version ?? 0) + 1,
        },
      },
    }
  })
}

export function syncKnowledgeComposerScope(tabId: string, scope: string) {
  useKnowledgeComposerContext.setState(({ entries }) => ({
    entries: {
      ...entries,
      [tabId]: {
        accountScope: scope,
        version:
          (entries[tabId]?.version ?? 0) +
          (entries[tabId]?.accountScope && entries[tabId].accountScope !== scope
            ? 1
            : 0),
        selection:
          entries[tabId]?.selection?.scope === scope
            ? entries[tabId].selection
            : undefined,
      },
    },
  }))
}

export function restoreKnowledgeComposerSelection(
  tabId: string,
  selection: KnowledgeSelection | undefined,
  expectedVersion?: number
) {
  const current = useKnowledgeComposerContext.getState().entries[tabId]
  if (
    selection &&
    !current?.selection?.selected.length &&
    (expectedVersion === undefined || current?.version === expectedVersion)
  )
    setKnowledgeComposerSelection(tabId, selection)
}
