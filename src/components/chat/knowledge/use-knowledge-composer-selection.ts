"use client"

import { useCallback, useEffect, useRef, useState } from "react"
import { useTranslations } from "next-intl"
import { toast } from "sonner"
import { useOptionalIywAccount } from "@/contexts/iyw-account-context"
import { useTabStore } from "@/contexts/tab-context"
import {
  getKnowledgeSelection,
  knowledgeError,
  saveKnowledgeSelection,
} from "@/lib/cloud-knowledge"
import {
  clearKnowledgeComposerSelection,
  setKnowledgeComposerSelection,
  useKnowledgeComposerContext,
} from "@/lib/knowledge-composer-context"

export function useKnowledgeComposerSelection(
  tabId: string | null | undefined
) {
  const t = useTranslations("CloudKnowledge")
  const account = useOptionalIywAccount()
  const accountKey = [
    account?.status,
    account?.profile?.user_id,
    account?.profile?.org_code,
  ].join(":")
  const conversationId = useTabStore((state) =>
    tabId
      ? (state.tabs.find((tab) => tab.id === tabId)?.conversationId ?? null)
      : null
  )
  const selection = useKnowledgeComposerContext((state) =>
    tabId ? state.entries[tabId]?.selection : undefined
  )
  const [busy, setBusy] = useState(false)
  useKnowledgeSelectionSync(tabId, conversationId, accountKey)

  const clear = useCallback(async () => {
    if (!tabId || !selection || busy) return
    if (!selection.selected.length || conversationId == null) {
      clearKnowledgeComposerSelection(tabId)
      return
    }
    setBusy(true)
    try {
      const next = await saveKnowledgeSelection(conversationId, {
        ...selection,
        selected: [],
      })
      setKnowledgeComposerSelection(tabId, next)
      toast.success(t("cleared"))
    } catch (cause) {
      toast.error(knowledgeError(cause))
    } finally {
      setBusy(false)
    }
  }, [busy, conversationId, selection, t, tabId])

  return { selection, busy, clear }
}

function useKnowledgeSelectionSync(
  tabId: string | null | undefined,
  conversationId: number | null,
  accountKey: string
) {
  const requestRef = useRef(0)
  useEffect(() => {
    if (!tabId) return
    const request = ++requestRef.current
    getKnowledgeSelection(conversationId)
      .then((value) => {
        if (requestRef.current === request)
          setKnowledgeComposerSelection(tabId, value)
      })
      .catch(() => {
        // The picker remains the source of truth when loading the chip fails.
      })
    return () => {
      requestRef.current = request + 1
    }
  }, [accountKey, conversationId, tabId])
}
