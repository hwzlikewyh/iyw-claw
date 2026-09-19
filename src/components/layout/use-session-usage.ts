"use client"

import { useCallback, useEffect, useState, useSyncExternalStore } from "react"
import {
  useOptionalConnectionStore,
  type ConnectionState,
} from "@/contexts/acp-connections-context"
import { useConversationRuntimeStore } from "@/stores/conversation-runtime-store"
import { useTabStore } from "@/contexts/tab-context"
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
  const activeTabKey = useTabStore((state) => state.activeTabId)
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
  const key =
    contextKey === undefined ? (activeKey ?? activeTabKey) : contextKey
  const subscribe = useCallback(
    (cb: () => void) => (key && store ? store.subscribeKey(key, cb) : () => {}),
    [key, store]
  )
  const snapshot = useCallback(
    () => (key && store ? store.getConnection(key) : undefined),
    [key, store]
  )
  const connection = useSyncExternalStore(subscribe, snapshot, snapshot)
  return { connection, contextKey: key }
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

function useUsageRuntimeSession(source: {
  sessionId?: string | null
  contextKey?: string | null
  agentType?: AgentType | null
}) {
  const tabConversationId = useTabStore((state) => {
    const tab = state.tabs.find((item) => item.id === source.contextKey)
    return !source.agentType || tab?.agentType === source.agentType
      ? tab?.conversationId
      : undefined
  })
  return useConversationRuntimeStore((state) => {
    const id =
      (source.sessionId
        ? state.conversationIdByExternalId.get(source.sessionId)
        : undefined) ?? tabConversationId
    return id == null ? undefined : state.byConversationId.get(id)
  })
}

function useConnectionStats(
  source: SessionUsageSourceProps & { connection?: ConnectionState },
  opened: boolean
) {
  const { connection } = source
  const session = useUsageRuntimeSession({
    sessionId: connection?.sessionId,
    contextKey: source.contextKey,
    agentType: source.agentType,
  })
  const baseline =
    source.sessionStats !== undefined
      ? source.sessionStats
      : (session?.sessionStats ?? null)
  const currentStats = useSessionUsageStats(
    {
      conversationId:
        session?.dbConversationId ?? session?.detail?.summary.id ?? null,
      sessionId: connection?.sessionId ?? session?.externalId ?? null,
      connectionId: connection?.connectionId ?? null,
      enabled: opened,
    },
    baseline
  )
  return {
    stats: currentStats ?? baseline,
    historyModel: session?.detail?.summary.model,
    historyAgent: session?.detail?.summary.agent_type,
  }
}

export function useSessionUsage(
  props: SessionUsageSourceProps,
  opened = false
) {
  const { connection: storedConnection, contextKey } = useUsageConnection(
    props.contextKey
  )
  const connection =
    props.agentType && storedConnection?.agentType !== props.agentType
      ? undefined
      : storedConnection
  const { stats, historyModel, historyAgent } = useConnectionStats(
    { ...props, contextKey, connection },
    opened
  )
  const agentType =
    connection?.agentType ?? props.agentType ?? historyAgent ?? null
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
