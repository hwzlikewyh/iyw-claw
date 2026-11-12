"use client"

import { useEffect, useRef, useState } from "react"
import { useTranslations } from "next-intl"
import { useTabStore } from "@/contexts/tab-context"
import { prepareConnectorConversation } from "@/lib/conversation-connectors"

export function usePluginWorkspace(tabId: string | null) {
  const t = useTranslations("PluginPreparation")
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState("")
  const alive = useRef(true)
  useEffect(() => {
    alive.current = true
    return () => {
      alive.current = false
    }
  }, [])
  const bind = async () => {
    if (!tabId || busy) return
    setBusy(true)
    setError("")
    try {
      const prepared = await prepareConnectorConversation(
        tabId,
        t("newConversation")
      )
      if (alive.current && useTabStore.getState().activeTabId === tabId)
        prepared.bind()
    } catch (cause) {
      if (alive.current) setError(String(cause))
    } finally {
      if (alive.current) setBusy(false)
    }
  }
  return { busy, error, bind }
}
