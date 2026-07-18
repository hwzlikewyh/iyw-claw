"use client"

import { useCallback, useEffect, useMemo, useState } from "react"
import { useTranslations } from "next-intl"
import { useIywAccount } from "@/contexts/iyw-account-context"

import {
  getFixedAgentOptions,
  loadFixedAgentOptions,
  refreshFixedAgentOptions,
} from "@/lib/fixed-agent-options"
import type { SessionConfigTranslator } from "@/lib/session-config-localization"
import type { AgentOptionsSnapshot, AgentType } from "@/lib/types"

export interface AgentOptionsState {
  snapshot: AgentOptionsSnapshot
  loading: false
  error: null
  reload: () => void
  ensure: () => Promise<AgentOptionsSnapshot>
}

/** Return the product-owned option catalog without launching an Agent. */
export function useAgentOptions(
  agentType: AgentType,
  _folderPath: string | null = null
): AgentOptionsState {
  void _folderPath
  const t = useTranslations("Folder.chat.messageInput")
  const { status: accountStatus } = useIywAccount()
  const translator = t as unknown as SessionConfigTranslator
  const [catalogVersion, setCatalogVersion] = useState(0)
  useEffect(() => {
    if (accountStatus !== "authenticated") return
    let active = true
    void loadFixedAgentOptions().then(() => {
      if (active) setCatalogVersion((version) => version + 1)
    })
    return () => {
      active = false
    }
  }, [accountStatus])
  const snapshot = useMemo(() => {
    void catalogVersion
    return getFixedAgentOptions(agentType, {}, translator)
  }, [agentType, translator, catalogVersion])
  const reload = useCallback(() => {
    void refreshFixedAgentOptions().then(() =>
      setCatalogVersion((version) => version + 1)
    )
  }, [])
  const ensure = useCallback(async () => snapshot, [snapshot])

  return { snapshot, loading: false, error: null, reload, ensure }
}
