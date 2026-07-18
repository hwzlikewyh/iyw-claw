"use client"

import { useCallback, useMemo } from "react"
import { useTranslations } from "next-intl"

import { getFixedAgentOptions } from "@/lib/fixed-agent-options"
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
  const translator = t as unknown as SessionConfigTranslator
  const snapshot = useMemo(
    () => getFixedAgentOptions(agentType, {}, translator),
    [agentType, translator]
  )
  const reload = useCallback(() => {}, [])
  const ensure = useCallback(async () => snapshot, [snapshot])

  return { snapshot, loading: false, error: null, reload, ensure }
}
