"use client"

import { useEffect, useRef, useState, type RefObject } from "react"
import { useTranslations } from "next-intl"
import { useTabStore } from "@/contexts/tab-context"
import { prepareConnectorConversation } from "@/lib/conversation-connectors"
import {
  getKnowledgeSelection,
  saveKnowledgeSelection,
  knowledgeChoiceKey,
  knowledgeError,
  KNOWLEDGE_SELECTION_LIMIT,
  type KnowledgeChoice,
  type KnowledgeSelection,
} from "@/lib/cloud-knowledge"

type PreparedConversation = Awaited<
  ReturnType<typeof prepareConnectorConversation>
>

export function useKnowledgeSelection(tabId: string, onSaved: () => void) {
  const t = useTranslations("CloudKnowledge")
  const conversationId = useTabStore(
    (state) =>
      state.tabs.find((tab) => tab.id === tabId)?.conversationId ?? null
  )
  const state = useSelectionRecord(conversationId)
  const { data, busy, setData, setError } = state
  const prepared = useRef<PreparedConversation | null>(null)
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
    conversationId,
    prepared,
    onSaved,
    title: t("newConversation"),
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

function useSelectionRecord(conversationId: number | null) {
  const [data, setData] = useState<KnowledgeSelection | null>(null)
  const [error, setError] = useState("")
  const [busy, setBusy] = useState(false)
  const [reload, setReload] = useState(0)
  const generation = useRef(0)
  useEffect(() => {
    const request = ++generation.current
    getKnowledgeSelection(conversationId)
      .then((value) => {
        if (generation.current === request) setData(value)
      })
      .catch((cause) => {
        if (generation.current === request) setError(knowledgeError(cause))
      })
    return () => {
      generation.current = request + 1
    }
  }, [conversationId, reload])
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
  conversationId: number | null
  data: KnowledgeSelection | null
  prepared: RefObject<PreparedConversation | null>
  generation: RefObject<number>
  busy: boolean
  setBusy: (busy: boolean) => void
  setError: (error: string) => void
  onSaved: () => void
  title: string
}

function useSaveSelection(context: SaveContext) {
  const saving = useRef(false)
  return async () => {
    const { data, generation, prepared } = context
    if (!data || context.busy || saving.current) return
    if (context.conversationId == null && !data.selected.length) {
      context.onSaved()
      return
    }
    saving.current = true
    const request = generation.current
    context.setBusy(true)
    context.setError("")
    try {
      const id = await prepareConversation(context)
      await saveKnowledgeSelection(id, data)
      if (
        generation.current !== request ||
        !conversationMatches(context.tabId, id)
      )
        return
      prepared.current?.bind()
      context.onSaved()
    } catch (cause) {
      if (generation.current === request)
        context.setError(knowledgeError(cause))
    } finally {
      saving.current = false
      if (generation.current === request) context.setBusy(false)
    }
  }
}

async function prepareConversation(context: SaveContext) {
  const { prepared, conversationId, tabId, title } = context
  if (conversationId == null && !prepared.current)
    prepared.current = await prepareConnectorConversation(tabId, title)
  const id = conversationId ?? prepared.current!.id
  if (!conversationMatches(tabId, id))
    throw new Error("Conversation changed; reopen the knowledge picker")
  return id
}

function conversationMatches(tabId: string, id: number) {
  const tab = useTabStore.getState().tabs.find((item) => item.id === tabId)
  return Boolean(
    tab && (tab.conversationId == null || tab.conversationId === id)
  )
}
