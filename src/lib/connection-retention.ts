import type { ConnectionStatus } from "@/lib/types"

export const CONVERSATION_COLD_AFTER_MS = 3 * 60 * 60 * 1000

export interface RetentionConnectionState {
  status: ConnectionStatus
  pendingPermission: unknown | null
  pendingQuestion: unknown | null
  pendingAskQuestion: unknown | null
  pendingChannelConfirmation: unknown | null
  backgroundOutstanding: number
  isDelegationChild: boolean
}

interface RetentionDecisionArgs {
  connection: RetentionConnectionState
  contextKey: string
  activeKey: string | null
  visibilityState: DocumentVisibilityState
  lastActiveAt: number | undefined
  now: number
}

export type DocumentVisibilityState = "hidden" | "visible"

export function isRetentionProtected(
  connection: RetentionConnectionState
): boolean {
  if (connection.status === "disconnected" || connection.status === "error") {
    return false
  }
  return (
    connection.status === "prompting" ||
    connection.status === "connecting" ||
    connection.pendingPermission != null ||
    connection.pendingQuestion != null ||
    connection.pendingAskQuestion != null ||
    connection.pendingChannelConfirmation != null ||
    connection.backgroundOutstanding > 0
  )
}

export function isVisibleActiveConversation(
  contextKey: string,
  activeKey: string | null,
  visibilityState: DocumentVisibilityState
): boolean {
  return visibilityState === "visible" && contextKey === activeKey
}

export function shouldKeepConnectionAlive(
  args: RetentionDecisionArgs
): boolean {
  if (args.connection.status !== "connected") return false
  if (isRetentionProtected(args.connection)) return true
  if (
    isVisibleActiveConversation(
      args.contextKey,
      args.activeKey,
      args.visibilityState
    )
  ) {
    return true
  }
  if (args.lastActiveAt == null) return true
  return args.now - args.lastActiveAt < CONVERSATION_COLD_AFTER_MS
}

export function shouldReclaimConnection(args: RetentionDecisionArgs): boolean {
  if (args.connection.isDelegationChild) return false
  if (isRetentionProtected(args.connection)) return false
  if (
    isVisibleActiveConversation(
      args.contextKey,
      args.activeKey,
      args.visibilityState
    )
  ) {
    return false
  }
  if (args.lastActiveAt == null) return false
  return args.now - args.lastActiveAt >= CONVERSATION_COLD_AFTER_MS
}
