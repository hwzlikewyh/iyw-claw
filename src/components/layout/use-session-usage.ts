"use client"

import { useCallback, useEffect, useState, useSyncExternalStore } from "react"
import {
  useOptionalConnectionStore,
  type ConnectionState,
} from "@/contexts/acp-connections-context"
import { useConversationRuntimeStore } from "@/stores/conversation-runtime-store"
import {
  getCachedGatewayModels,
  getGatewayModels,
  refreshGatewayModels,
} from "@/lib/gateway-model-catalog"
import { isModelConfigOption } from "@/lib/model-config-groups"
import {
  findUsageModel,
  resolveSessionUsage,
} from "@/lib/session-usage-display"
import type { AgentType, SessionStats } from "@/lib/types"
import type { GatewayModel } from "@/lib/gateway-model-parser"
import { useSessionUsageStats } from "./use-session-usage-stats"

export interface SessionUsageSourceProps {
  contextKey?: string | null
  sessionStats?: SessionStats | null
  agentType?: AgentType | null
  modelId?: string | null
}

function useUsageConnection(contextKey: string | null | undefined) {
  const store = useOptionalConnectionStore()
  const subscribeActive = useCallback(
    (cb: () => void) => store?.subscribeActiveKey(cb) ?? (() => {}),
    [store]
  )
  const activeSnapshot = useCallback(
    () => store?.getActiveKey() ?? null,
    [store]
  )
  const activeKey = useSyncExternalStore(
    subscribeActive,
    activeSnapshot,
    activeSnapshot
  )
  const key = contextKey === undefined ? activeKey : contextKey
  const subscribe = useCallback(
    (cb: () => void) => (key && store ? store.subscribeKey(key, cb) : () => {}),
    [key, store]
  )
  const snapshot = useCallback(
    () => (key && store ? store.getConnection(key) : undefined),
    [key, store]
  )
  const connection = useSyncExternalStore(subscribe, snapshot, snapshot)
  return connection
}

function useUsageModels(agentType: AgentType | null) {
  const [snapshot, setSnapshot] = useState<{
    agent: AgentType
    models: GatewayModel[]
  } | null>(null)
  useEffect(() => {
    if (!agentType) return
    let cancelled = false
    void getGatewayModels(agentType).then((models) => {
      if (!cancelled) setSnapshot({ agent: agentType, models })
    })
    return () => {
      cancelled = true
    }
  }, [agentType])
  const refresh = useCallback(() => {
    if (!agentType) return
    void refreshGatewayModels(agentType).then((models) =>
      setSnapshot({ agent: agentType, models })
    )
  }, [agentType])
  const models =
    snapshot?.agent === agentType
      ? snapshot.models
      : getCachedGatewayModels(agentType ?? undefined)
  return { models, refresh }
}

function useUsageRuntimeSession(sessionId: string | null | undefined) {
  return useConversationRuntimeStore((state) => {
    const id = sessionId
      ? state.conversationIdByExternalId.get(sessionId)
      : undefined
    return id === undefined ? undefined : state.byConversationId.get(id)
  })
}

function useConnectionStats(
  connection: ConnectionState | undefined,
  override: SessionStats | null | undefined,
  opened: boolean
) {
  const session = useUsageRuntimeSession(connection?.sessionId)
  const baseline =
    override !== undefined ? override : (session?.sessionStats ?? null)
  const currentStats = useSessionUsageStats(
    {
      conversationId: session?.dbConversationId ?? null,
      sessionId: connection?.sessionId ?? null,
      connectionId: connection?.connectionId ?? null,
      enabled: opened,
    },
    baseline
  )
  return {
    stats: currentStats ?? baseline,
    historyModel: session?.detail?.summary.model,
  }
}

export function useSessionUsage(
  props: SessionUsageSourceProps,
  opened = false
) {
  const storedConnection = useUsageConnection(props.contextKey)
  const connection =
    props.agentType && storedConnection?.agentType !== props.agentType
      ? undefined
      : storedConnection
  const { stats, historyModel } = useConnectionStats(
    connection,
    props.sessionStats,
    opened
  )
  const agentType = connection?.agentType ?? props.agentType ?? null
  const modelId =
    connection?.configOptions?.find(isModelConfigOption)?.kind.current_value ??
    historyModel ??
    props.modelId ??
    null
  const { models, refresh } = useUsageModels(agentType)
  const data = resolveSessionUsage({
    agentType,
    modelId,
    stats,
    model: findUsageModel(models, modelId),
    liveUsage: connection?.usage,
    hostThreshold: connection?.compactionAtTokens,
    configStale: connection?.configStale,
  })
  return {
    data,
    refresh,
    visible: agentType !== null || stats !== null || connection?.usage != null,
  }
}
