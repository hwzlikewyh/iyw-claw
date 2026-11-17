"use client"

import { useEffect, useRef, useState } from "react"
import { useTranslations } from "next-intl"
import {
  getKnowledgeSelection,
  knowledgeChoiceKey,
  knowledgeError,
  KNOWLEDGE_SELECTION_LIMIT,
  type KnowledgeChoice,
  type KnowledgeSelection,
} from "@/lib/cloud-knowledge"
import {
  setKnowledgeComposerSelection,
  syncKnowledgeComposerScope,
  useKnowledgeComposerContext,
} from "@/lib/knowledge-composer-context"

export function useKnowledgeSelection(
  tabId: string,
  onSaved: (selection: KnowledgeSelection) => void
) {
  const t = useTranslations("CloudKnowledge")
  const state = useSelectionRecord(tabId)
  const { data, busy, setData, setError } = state
  const toggle = (choice: KnowledgeChoice) => {
    if (!data || busy) return
    const key = knowledgeChoiceKey(choice)
    const exists = data.selected.some(
      (item) => knowledgeChoiceKey(item) === key
    )
    if (!exists && data.selected.length >= KNOWLEDGE_SELECTION_LIMIT) {
      setError(t("limit", { count: KNOWLEDGE_SELECTION_LIMIT }))
      return
    }
    setError("")
    setData({
      ...data,
      selected: exists
        ? data.selected.filter((item) => knowledgeChoiceKey(item) !== key)
        : [...data.selected, choice],
    })
  }
  const save = useSaveSelection({
    ...state,
    tabId,
    onSaved,
  })
  const clear = () => {
    if (data && !busy) setData({ ...data, selected: [] })
    setError("")
  }
  return {
    data,
    error: state.error,
    busy,
    toggle,
    save,
    clear,
    retry: state.retry,
  }
}

function useSelectionRecord(tabId: string) {
  const [data, setData] = useState<KnowledgeSelection | null>(null)
  const [error, setError] = useState("")
  const [busy, setBusy] = useState(false)
  const [reload, setReload] = useState(0)
  const generation = useRef(0)
  useEffect(() => {
    const request = ++generation.current
    getKnowledgeSelection(null)
      .then((value) => {
        if (generation.current !== request) return
        syncKnowledgeComposerScope(tabId, value.scope)
        setData(
          useKnowledgeComposerContext.getState().entries[tabId]?.selection ??
            value
        )
      })
      .catch((cause) => {
        if (generation.current === request) setError(knowledgeError(cause))
      })
    return () => {
      generation.current = request + 1
    }
  }, [tabId, reload])
  return {
    data,
    setData,
    error,
    setError,
    busy,
    setBusy,
    generation,
    retry: () => {
      setData(null)
      setError("")
      setBusy(false)
      setReload((value) => value + 1)
    },
  }
}

interface SaveContext {
  tabId: string
  data: KnowledgeSelection | null
  generation: { current: number }
  busy: boolean
  setBusy: (busy: boolean) => void
  setError: (error: string) => void
  onSaved: (selection: KnowledgeSelection) => void
}

function useSaveSelection(context: SaveContext) {
  const saving = useRef(false)
  return async () => {
    const { data, generation } = context
    if (!data || context.busy || saving.current) return
    saving.current = true
    const request = generation.current
    context.setBusy(true)
    context.setError("")
    try {
      const current = await getKnowledgeSelection(null)
      if (generation.current !== request) return
      syncKnowledgeComposerScope(context.tabId, current.scope)
      if (current.scope !== data.scope)
        throw new Error("账号或空间已变更，请重新打开知识库")
      setKnowledgeComposerSelection(context.tabId, data)
      context.onSaved(data)
    } catch (cause) {
      if (generation.current === request)
        context.setError(knowledgeError(cause))
    } finally {
      saving.current = false
      if (generation.current === request) context.setBusy(false)
    }
  }
}
