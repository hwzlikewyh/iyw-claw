import { onTransportChange } from "@/lib/transport"
import type { UIValue } from "./engine/expression"
import { validState } from "./state-value"

type State = Record<string, UIValue>
export const EMPTY_DRAFT = {
  state: {} as State,
  changed: false,
  error: null as string | null,
}
type Draft = typeof EMPTY_DRAFT
const drafts = new Map<string, Draft>()
const listeners = new Map<string, Set<() => void>>()
export const draftKey = (conversationId: number, id: string) =>
  `${conversationId}:${id}`
export const draftSnapshot = (key: string) => drafts.get(key) ?? EMPTY_DRAFT
const notify = (key: string) =>
  listeners.get(key)?.forEach((listener) => listener())
export function subscribeDraft(key: string, listener: () => void) {
  const set = listeners.get(key) ?? new Set()
  set.add(listener)
  listeners.set(key, set)
  return () => {
    set.delete(listener)
    if (!set.size) listeners.delete(key)
  }
}
export function updateDraft(key: string, state: State) {
  const current = draftSnapshot(key)
  if (validState(state)) drafts.set(key, { state, changed: true, error: null })
  else
    drafts.set(key, {
      ...current,
      error: "Invalid state or state exceeds 4KiB",
    })
  notify(key)
}
export function clearDraft(key: string) {
  drafts.delete(key)
  notify(key)
}
onTransportChange(() => {
  drafts.clear()
  for (const key of listeners.keys()) notify(key)
})
