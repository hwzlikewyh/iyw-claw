"use client"

import { useCallback, useEffect, useMemo, useSyncExternalStore } from "react"
import {
  type BlockSnapshot,
  blockKey,
  blockSnapshot,
  loadBlock,
  subscribeBlock,
  updateBlock,
  retryBlock,
} from "@/lib/intelligent-ui/state-store"
import type { UIValue } from "@/lib/intelligent-ui/engine/expression"
import { mergeState, stateDiff } from "@/lib/intelligent-ui/engine/block"
import {
  draftKey,
  draftSnapshot,
  subscribeDraft,
  updateDraft,
  clearDraft,
} from "@/lib/intelligent-ui/draft-store"
import { sourceHash } from "@/lib/intelligent-ui/hash"
import { useUiScope } from "./scope"

const EMPTY = { state: {}, revision: 0, loaded: false, error: null }
type State = Record<string, UIValue>
type Input = {
  id: string
  source: string
  defaults: Readonly<State>
  freeState?: boolean
}
const viewState = (state: Readonly<Record<string, unknown>>) =>
  Object.fromEntries(
    Object.entries(state).filter(([name]) => name.startsWith("__view_"))
  ) as State
export function useBlockState(input: Input) {
  const { scope, key, saved } = useSavedBlock(input)
  const draftId = draftKey(scope?.conversationId ?? 0, input.id)
  const draft = useSyncExternalStore(
    useCallback((fn) => subscribeDraft(draftId, fn), [draftId]),
    () => draftSnapshot(draftId),
    () => draftSnapshot("")
  )
  useDraftTransfer({
    key,
    draftId,
    saved,
    streaming: Boolean(scope?.streaming),
  })
  const overrides = selectOverrides(draft, saved)
  const state = useMemo(
    () =>
      input.freeState
        ? (overrides as State)
        : { ...mergeState(input.defaults, overrides), ...viewState(overrides) },
    [input.defaults, overrides, input.freeState]
  )
  function update(patch: State | null) {
    const transient = draftSnapshot(draftId)
    const current = (
      scope?.streaming || !saved.loaded || saved.revision < 0
        ? transient.state
        : (blockSnapshot(key)?.state ?? {})
    ) as State
    const next = delta(
      { ...input.defaults, ...current, ...patch },
      patch,
      input
    )
    if (saved.loaded && saved.revision >= 0 && !scope?.streaming)
      updateBlock(key, next)
    else updateDraft(draftId, next)
  }
  return {
    state,
    update,
    error: saved.error ?? draft.error,
    ready: Boolean(scope?.enabled && input.id),
    retry: (reload = false) => retryBlock(key, reload),
  }
}
function selectOverrides(
  draft: ReturnType<typeof draftSnapshot>,
  saved: BlockSnapshot
) {
  return draft.changed || !saved.loaded ? draft.state : saved.state
}
function delta(merged: State, patch: State | null, input: Input): State {
  if (patch === null) return {}
  if (input.freeState) return merged
  return {
    ...stateDiff(input.defaults, mergeState(input.defaults, merged)),
    ...viewState(merged),
  }
}
function useSavedBlock(input: Input) {
  const scope = useUiScope()
  const hash = useMemo(
    () => (scope?.streaming ? "" : sourceHash(input.source)),
    [input.source, scope?.streaming]
  )
  const persistedId = scope?.persistedConversationId
  const key = blockKey(persistedId ?? 0, input.id, hash)
  const saved = useSyncExternalStore(
    useCallback((fn) => subscribeBlock(key, fn), [key]),
    () => blockSnapshot(key) ?? EMPTY,
    () => EMPTY
  )
  useEffect(() => {
    if (
      !hash ||
      !input.id ||
      !scope?.enabled ||
      saved.loaded ||
      scope.streaming ||
      persistedId == null
    )
      return
    void loadBlock({
      conversationId: persistedId,
      blockId: input.id,
      source: input.source,
      hash,
    })
  }, [
    hash,
    input.id,
    input.source,
    persistedId,
    scope?.streaming,
    scope?.enabled,
    saved.loaded,
  ])
  return { scope, key, saved }
}
function useDraftTransfer(input: {
  key: string
  draftId: string
  saved: BlockSnapshot
  streaming: boolean
}) {
  useEffect(() => {
    const draft = draftSnapshot(input.draftId)
    if (
      !input.saved.loaded ||
      input.saved.revision < 0 ||
      !draft.changed ||
      input.streaming
    )
      return
    updateBlock(input.key, draft.state)
    clearDraft(input.draftId)
  }, [
    input.saved.loaded,
    input.saved.revision,
    input.saved.state,
    input.streaming,
    input.key,
    input.draftId,
  ])
}
