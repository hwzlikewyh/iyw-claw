"use client"

import { createContext, useContext, useMemo, type ReactNode } from "react"
import type { AgentInputItem, ConnectionStatus } from "@/lib/types"

const EMPTY_INPUTS: AgentInputItem[] = []
const PENDING = new Set(["waiting", "dispatching", "fallback_queued", "failed"])

interface DeliveryActions {
  onDelete?: (id: string) => void
  onRetry?: (id: string) => void
  onReorder?: (ids: string[]) => Promise<void>
  onForceThrough?: (id: string, prefix: string[]) => void
  onEdit?: (id: string) => void
  onSendNow?: (id: string) => void
}
interface DeliveryState extends DeliveryActions {
  byId: Map<string, AgentInputItem>
  pending: AgentInputItem[]
  online: boolean
}
const DeliveryContext = createContext<DeliveryState | null>(null)

export function AgentInputDeliveryProvider({
  items = EMPTY_INPUTS,
  status,
  children,
  ...actions
}: DeliveryActions & {
  items?: AgentInputItem[]
  status: ConnectionStatus | null
  children: ReactNode
}) {
  const { onDelete, onRetry, onReorder, onForceThrough, onSendNow, onEdit } =
    actions
  const value = useMemo(
    () => ({
      byId: new Map(items.map((item) => [item.id, item])),
      pending: items.filter((item) => PENDING.has(item.status)),
      online: status === "connected" || status === "prompting",
      onDelete,
      onRetry,
      onReorder,
      onForceThrough,
      onEdit,
      onSendNow,
    }),
    [
      items,
      status,
      onDelete,
      onRetry,
      onReorder,
      onForceThrough,
      onSendNow,
      onEdit,
    ]
  )
  return (
    <DeliveryContext.Provider value={value}>
      {children}
    </DeliveryContext.Provider>
  )
}

export function useAgentInputDelivery() {
  return useContext(DeliveryContext)
}

export function inputDeliveryKey(item: AgentInputItem, online = true) {
  if (
    !online &&
    ["waiting", "dispatching", "fallback_queued"].includes(item.status)
  )
    return "inputSyncing"
  if (item.status === "waiting" && item.force_requested_at)
    return "inputPrioritized"
  if (item.status === "waiting" && item.payload.delivery === "queue")
    return "inputQueued"
  const keys = {
    waiting: "inputWaiting",
    dispatching: "inputSubmitting",
    fallback_queued: "inputQueued",
    consumed: "inputDelivered",
    failed: "inputFailed",
    deleted: "inputDeleted",
  } as const
  return keys[item.status]
}
