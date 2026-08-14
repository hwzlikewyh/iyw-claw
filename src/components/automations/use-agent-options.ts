"use client"

import { useCallback, useEffect, useMemo, useState } from "react"
import { useTranslations } from "next-intl"
import { useIywAccount } from "@/contexts/iyw-account-context"

import {
  getFixedAgentOptions,
  hasAuthoritativeFixedAgentOptions,
  loadFixedAgentOptions,
  refreshFixedAgentOptions,
} from "@/lib/fixed-agent-options"
import type { SessionConfigTranslator } from "@/lib/session-config-localization"
import type { AgentOptionsSnapshot, AgentType } from "@/lib/types"

export interface AgentOptionsState {
  snapshot: AgentOptionsSnapshot
  loading: false
  error: null
  authoritative: boolean
  reload: () => void
  ensure: () => Promise<AgentOptionsSnapshot>
}

/** Return the product-owned option catalog without launching an Agent. */
export function useAgentOptions(
  agentType: AgentType,
  _folderPath: string | null = null,
  configValues: Record<string, string> = {}
): AgentOptionsState {
  void _folderPath
  const t = useTranslations("Folder.chat.messageInput")
  const { status: accountStatus } = useIywAccount()
  const translator = t as unknown as SessionConfigTranslator
  const [catalogVersion, setCatalogVersion] = useState(0)
  useEffect(() => {
    if (accountStatus !== "authenticated") return
    let active = true
    void loadFixedAgentOptions(agentType).then(() => {
      if (active) setCatalogVersion((version) => version + 1)
    })
    return () => {
      active = false
    }
  }, [accountStatus, agentType])
  const snapshot = useMemo(() => {
    void catalogVersion
    return getFixedAgentOptions(agentType, configValues, translator)
  }, [agentType, configValues, translator, catalogVersion])
  const authoritative = hasAuthoritativeFixedAgentOptions(agentType)
  const reload = useCallback(() => {
    void refreshFixedAgentOptions(agentType).then(() =>
      setCatalogVersion((version) => version + 1)
    )
  }, [agentType])
  const ensure = useCallback(async () => snapshot, [snapshot])

  return {
    snapshot,
    loading: false,
    error: null,
    authoritative,
    reload,
    ensure,
  }
}
