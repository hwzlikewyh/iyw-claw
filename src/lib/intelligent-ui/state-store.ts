import { readUiStates, saveUiState, type UiState } from "./api"
import { validState } from "./state-value"
import { onTransportChange } from "@/lib/transport"
import { uiEnabled } from "./settings"
import { toErrorMessage } from "@/lib/app-error"

export interface BlockSnapshot {
  state: Record<string, unknown>
  revision: number
  loaded: boolean
  error: string | null
}
type Entry = {
  snapshot: BlockSnapshot
  source: string
  blockId: string
  conversationId: number
  dirty: boolean
  pending?: Promise<void>
  timer?: ReturnType<typeof setTimeout>
}
const entries = new Map<string, Entry>()
const listeners = new Map<string, Set<() => void>>()
const sessions = new Map<number, Promise<UiState[]>>()
const DEBOUNCE_MS = 500

export function blockKey(
  conversationId: number,
  blockId: string,
  hash: string
) {
  return `${conversationId}:${blockId}:${hash}`
}
function notify(key: string) {
  listeners.get(key)?.forEach((fn) => fn())
}

export function subscribeBlock(key: string, listener: () => void) {
  const set = listeners.get(key) ?? new Set()
  set.add(listener)
  listeners.set(key, set)
  return () => {
    set.delete(listener)
    if (!set.size) listeners.delete(key)
  }
}
export function blockSnapshot(key: string) {
  return entries.get(key)?.snapshot
}

export async function loadBlock(input: {
  conversationId: number
  blockId: string
  source: string
  hash: string
}) {
  const key = blockKey(input.conversationId, input.blockId, input.hash)
  if (entries.has(key)) return
  const entry: Entry = {
    ...input,
    snapshot: { state: {}, revision: 0, loaded: false, error: null },
    dirty: false,
  }
  entries.set(key, entry)
  notify(key)
  try {
    const load =
      sessions.get(input.conversationId) ?? readUiStates(input.conversationId)
    sessions.set(input.conversationId, load)
    const rows = await load
    if (entries.get(key) !== entry) return
    const row = rows.find(
      (item) => item.blockId === input.blockId && item.sourceHash === input.hash
    )
    if (!entry.dirty)
      entry.snapshot = {
        state: row ? JSON.parse(row.stateJson) : {},
        revision: row?.revision ?? 0,
        loaded: true,
        error: null,
      }
  } catch (error) {
    sessions.delete(input.conversationId)
    if (entries.get(key) !== entry) return
    entry.snapshot = {
      ...entry.snapshot,
      revision: -1,
      loaded: true,
      error: toErrorMessage(error),
    }
  }
  notify(key)
}

export function updateBlock(key: string, state: Record<string, unknown>) {
  const entry = entries.get(key)
  if (!entry?.snapshot.loaded || entry.snapshot.revision < 0) return
  if (!validState(state)) {
    entry.snapshot = {
      ...entry.snapshot,
      error: "Invalid state or state exceeds 4KiB",
    }
    notify(key)
    return
  }
  if (JSON.stringify(state) === JSON.stringify(entry.snapshot.state)) return
  entry.snapshot = { ...entry.snapshot, state, error: null }
  entry.dirty = true
  if (entry.timer) clearTimeout(entry.timer)
  entry.timer = setTimeout(
    () => void persistBlock(key).catch(() => {}),
    DEBOUNCE_MS
  )
  notify(key)
}

export async function persistBlock(key: string): Promise<void> {
  const entry = entries.get(key)
  if (!entry || !entry.dirty) return
  if (!uiEnabled()) return
  if (entry.pending) {
    await entry.pending
    return persistBlock(key)
  }
  if (entry.timer) clearTimeout(entry.timer)
  const captured = entry.snapshot.state
  entry.pending = saveUiState({
    conversationId: entry.conversationId,
    blockId: entry.blockId,
    source: entry.source,
    state: captured,
    revision: entry.snapshot.revision,
  })
    .then((row) => {
      if (entries.get(key) !== entry) return
      entry.dirty = entry.snapshot.state !== captured
      entry.snapshot = {
        ...entry.snapshot,
        revision: row.revision,
        error: null,
      }
      sessions.delete(entry.conversationId)
    })
    .catch((error) => {
      if (entries.get(key) !== entry) return
      entry.snapshot = { ...entry.snapshot, error: toErrorMessage(error) }
      throw error
    })
    .finally(() => {
      entry.pending = undefined
      notify(key)
    })
  await entry.pending
  if (entries.get(key) === entry && entry.dirty) await persistBlock(key)
}

export async function flushUiStates(conversationId: number) {
  if (!uiEnabled()) return
  const keys = [...entries]
    .filter(
      ([, entry]) => entry.conversationId === conversationId && entry.dirty
    )
    .map(([key]) => key)
  await Promise.all(keys.map(persistBlock))
}

export async function flushAllUiStates() {
  if (!uiEnabled()) return
  await Promise.all(
    [...entries]
      .filter(([, entry]) => entry.dirty)
      .map(([key]) => persistBlock(key))
  )
}

export function acceptRemoteState(row: UiState) {
  for (const [key, entry] of entries) {
    if (
      entry.conversationId !== row.conversationId ||
      entry.blockId !== row.blockId
    )
      continue
    if (
      entry.dirty ||
      row.sourceHash !== key.slice(key.lastIndexOf(":") + 1) ||
      row.revision <= entry.snapshot.revision
    )
      continue
    entry.snapshot = {
      loaded: true,
      state: JSON.parse(row.stateJson),
      revision: row.revision,
      error: null,
    }
    notify(key)
  }
  sessions.delete(row.conversationId)
}

export function invalidateUiStates() {
  sessions.clear()
  for (const [key, entry] of entries) {
    if (entry.dirty || entry.pending) continue
    entries.delete(key)
    notify(key)
  }
}

export function clearUiStates() {
  for (const entry of entries.values())
    if (entry.timer) clearTimeout(entry.timer)
  entries.clear()
  sessions.clear()
  for (const key of listeners.keys()) notify(key)
}
onTransportChange(clearUiStates)

export async function retryBlock(key: string, reload = false) {
  const entry = entries.get(key)
  if (!entry) return
  if (reload) {
    sessions.delete(entry.conversationId)
    entries.delete(key)
    await loadBlock({
      conversationId: entry.conversationId,
      blockId: entry.blockId,
      source: entry.source,
      hash: key.slice(key.lastIndexOf(":") + 1),
    })
    return
  }
  if (entry.snapshot.error && !entry.dirty) return retryBlock(key, true)
  await persistBlock(key)
}
