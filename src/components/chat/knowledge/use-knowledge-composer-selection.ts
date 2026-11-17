"use client"

import { useCallback, useEffect, useRef } from "react"
import { useTranslations } from "next-intl"
import { toast } from "sonner"
import { useOptionalIywAccount } from "@/contexts/iyw-account-context"
import { getKnowledgeSelection } from "@/lib/cloud-knowledge"
import {
  clearKnowledgeComposerSelection,
  restoreKnowledgeComposerSelection,
  syncKnowledgeComposerScope,
  useKnowledgeComposerContext,
} from "@/lib/knowledge-composer-context"

export function useKnowledgeQueueDraft(
  tabId: string | null | undefined,
  editing: boolean | undefined
) {
  useEffect(() => {
    if (!tabId || !editing) return
    const previous =
      useKnowledgeComposerContext.getState().entries[tabId]?.selection
    return () => {
      clearKnowledgeComposerSelection(tabId)
      restoreKnowledgeComposerSelection(tabId, previous)
    }
  }, [editing, tabId])
}

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
  const selection = useKnowledgeComposerContext((state) =>
    tabId ? state.entries[tabId]?.selection : undefined
  )
  useKnowledgeSelectionSync(tabId, accountKey)

  const clear = useCallback(() => {
    if (!tabId || !selection) return
    clearKnowledgeComposerSelection(tabId)
    toast.success(t("cleared"))
  }, [selection, t, tabId])

  return { selection, busy: false, clear }
}

function useKnowledgeSelectionSync(
  tabId: string | null | undefined,
  accountKey: string
) {
  const requestRef = useRef(0)
  useEffect(() => {
    if (!tabId) return
    const request = ++requestRef.current
    getKnowledgeSelection(null)
      .then((value) => {
        if (requestRef.current === request)
          syncKnowledgeComposerScope(tabId, value.scope)
      })
      .catch(() => {
        // The picker remains the source of truth when loading the chip fails.
      })
    return () => {
      requestRef.current = request + 1
    }
  }, [accountKey, tabId])
}
