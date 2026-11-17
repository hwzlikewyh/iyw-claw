"use client"

import { useEffect, useState } from "react"
import type { AgentType } from "@/lib/types"
import {
  getCachedGatewayModels,
  getGatewayModels,
} from "@/lib/gateway-model-catalog"
import type { GatewayModel } from "@/lib/gateway-model-parser"

export function useAgentModelCatalog(agentType: AgentType): GatewayModel[] {
  const [snapshot, setSnapshot] = useState<{
    agentType: AgentType
    models: GatewayModel[]
  } | null>(null)
  const cachedModels = getCachedGatewayModels(agentType)

  useEffect(() => {
    let active = true
    void getGatewayModels(agentType).then((models) => {
      if (active) setSnapshot({ agentType, models })
    })
    return () => {
      active = false
    }
  }, [agentType])

  return snapshot?.agentType === agentType ? snapshot.models : cachedModels
}
