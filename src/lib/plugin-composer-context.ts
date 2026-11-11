import { create } from "zustand"
import type { PluginPanelChoice } from "./plugin-panel"
import {
  parsePluginPromptInstruction,
  pluginPromptInstruction,
} from "./plugin-prompt"
import type { PromptDraft } from "./types"

interface PluginComposerContext {
  preparedConversationId?: number
  selected?: PluginPanelChoice
}

export const usePluginComposerContext = create<{
  entries: Record<string, PluginComposerContext>
}>(() => ({ entries: {} }))

function update(tabId: string, patch: Partial<PluginComposerContext>) {
  usePluginComposerContext.setState(({ entries }) => ({
    entries: { ...entries, [tabId]: { ...entries[tabId], ...patch } },
  }))
}

export function preparePluginDraft(tabId: string, conversationId: number) {
  update(tabId, { preparedConversationId: conversationId })
}

export function selectComposerPlugin(
  tabId: string,
  selected: PluginPanelChoice
) {
  update(tabId, { selected })
}

export function clearComposerPlugin(tabId: string) {
  update(tabId, { selected: undefined })
}

export function clearPluginComposerContext(tabId?: string) {
  if (!tabId) return usePluginComposerContext.setState({ entries: {} })
  const entries = { ...usePluginComposerContext.getState().entries }
  delete entries[tabId]
  usePluginComposerContext.setState({ entries })
}

export function pluginDraftVisible(input: {
  tabId: string
  conversationId: number | null
  sent: boolean
  hasTurns: boolean
}) {
  const id =
    usePluginComposerContext.getState().entries[input.tabId]
      ?.preparedConversationId
  return (
    id != null && id === input.conversationId && !input.sent && !input.hasTurns
  )
}

export function pluginPrompt(
  tabId: string | null | undefined,
  draft: PromptDraft
): PromptDraft {
  const selected =
    tabId && usePluginComposerContext.getState().entries[tabId]?.selected
  if (!selected) return draft
  const instruction = pluginPromptInstruction(selected)
  const [first, ...remaining] = draft.blocks
  const previous =
    first?.type === "text" ? parsePluginPromptInstruction(first.text) : null
  const blocks = previous
    ? [
        ...(previous.remainder
          ? [{ type: "text" as const, text: previous.remainder }]
          : []),
        ...remaining,
      ]
    : draft.blocks
  return {
    ...draft,
    blocks: [{ type: "text", text: instruction }, ...blocks],
  }
}
