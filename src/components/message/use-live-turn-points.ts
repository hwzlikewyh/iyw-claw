"use client"

import { useEffect, useState } from "react"
import type { LiveMessage } from "@/contexts/acp-connections-context"
import {
  getCachedGatewayModels,
  getGatewayModels,
} from "@/lib/gateway-model-catalog"
import type { GatewayModel } from "@/lib/gateway-model-parser"
import { findUsageModel } from "@/lib/session-usage-display"
import { estimateLiveUsagePoints } from "@/lib/live-usage-points"
import type { AgentType, TurnUsage } from "@/lib/types"

interface LiveTurnPointsInput {
  agentType: AgentType
  enabled: boolean
  message: LiveMessage | null
  modelId: string | null
  promptText: string
  usage?: TurnUsage | null
}

export function useLiveTurnPoints({
  agentType,
  enabled,
  message,
  modelId,
  promptText,
  usage,
}: LiveTurnPointsInput): number | null {
  const [catalog, setCatalog] = useState<{
    agentType: AgentType
    models: GatewayModel[]
  } | null>(null)
  const cached = getCachedGatewayModels(agentType)
  useEffect(() => {
    if (!enabled) return
    let active = true
    void getGatewayModels(agentType).then((models) => {
      if (active) setCatalog({ agentType, models })
    })
    return () => {
      active = false
    }
  }, [agentType, enabled])
  const models = catalog?.agentType === agentType ? catalog.models : cached
  const model = findUsageModel(models, modelId)
  return enabled && message
    ? estimateLiveUsagePoints({
        message,
        promptText,
        modelPrice: model?.tokenPrice ?? null,
        usage,
      })
    : null
}
