"use client"

import { useEffect, useMemo, useState } from "react"
import { useIywAccount } from "@/contexts/iyw-account-context"
import { getTransport, onTransportChange } from "@/lib/transport"
import { onSkillMarketInstallationsChanged } from "@/lib/skill-market-installation-events"
import { toErrorMessage } from "@/lib/app-error"
import {
  enrichPluginChoices,
  inventoryPluginChoices,
  loadComposerPluginCatalog,
  loadComposerPluginInventory,
  type ComposerPluginChoice,
} from "@/lib/composer-plugins"
import type { AgentType } from "@/lib/types"
import type { SkillMarketV2Item } from "@/lib/skill-market"

export function useComposerPlugins(input: {
  open: boolean
  agentType: AgentType | null
  workspacePath: string | null
}) {
  const { profile, status } = useIywAccount()
  const [revision, setRevision] = useState(0)
  const [state, setState] = useState({
    key: "",
    choices: [] as ComposerPluginChoice[],
    loading: true,
    error: "",
  })
  const key = JSON.stringify([
    input.agentType,
    input.workspacePath,
    profile?.user_id,
    profile?.org_code,
    profile?.workspace_pending,
    status,
    revision,
  ])
  const transport = getTransport()
  useEffect(
    () => onTransportChange(() => setRevision((value) => value + 1)),
    []
  )
  useEffect(() => {
    const refresh = () => setRevision((value) => value + 1)
    window.addEventListener("focus", refresh)
    const unsubscribe = onSkillMarketInstallationsChanged(refresh)
    return () => {
      window.removeEventListener("focus", refresh)
      unsubscribe()
    }
  }, [])
  useEffect(() => {
    if (!input.open || !input.agentType) return
    let alive = true
    let local: ComposerPluginChoice[] | null = null
    let catalog: SkillMarketV2Item[] = []
    const publish = () => {
      if (alive && local)
        setState({
          key,
          choices: enrichPluginChoices(local, catalog),
          loading: false,
          error: "",
        })
    }
    const agentType = input.agentType
    void loadComposerPluginInventory(transport, {
      agentType,
      workspacePath: input.workspacePath,
    })
      .then((data) => {
        local = inventoryPluginChoices(data, agentType)
        publish()
      })
      .catch((cause) => {
        if (alive)
          setState({
            key,
            choices: [],
            loading: false,
            error: toErrorMessage(cause),
          })
      })
    void loadComposerPluginCatalog()
      .then((items) => {
        catalog = items
        publish()
      })
      .catch(() => {
        /* 本地库存不依赖市场在线。 */
      })
    return () => {
      alive = false
    }
  }, [input.open, input.agentType, input.workspacePath, key, transport])
  return useMemo(() => {
    const current =
      state.key === key
        ? state
        : { choices: [] as ComposerPluginChoice[], loading: true, error: "" }
    return {
      ...current,
      scopeKey: JSON.stringify([
        input.agentType,
        profile?.user_id,
        profile?.org_code,
        status,
      ]),
      refresh: () => setRevision((value) => value + 1),
    }
  }, [key, state, input.agentType, profile?.user_id, profile?.org_code, status])
}
