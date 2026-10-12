import { getTransport } from "@/lib/transport"
import type {
  AgentType,
  ChatChannelMessageLog,
  LiveSessionSnapshot,
} from "@/lib/types"
import type messages from "@/i18n/messages/en.json"

export type ChannelPhase =
  keyof typeof messages.ChatChannelSettings.runtime.phases

export interface ChannelConversationInfo {
  channelId: number
  conversationId: number
  folderId: number
  title: string | null
  agentType: AgentType
  model: string | null
  messageCount: number
  updatedAt: string
  phase: ChannelPhase
  startedAt: string | null
  lastActivityAt: string | null
  currentTool: string | null
  error: string | null
  connectionId: string | null
}

export interface ChannelConversationPage {
  items: ChannelConversationInfo[]
  nextBefore: ConversationCursor | null
  sampledAt: string
}

export interface ChannelConversationRuntime {
  conversation: ChannelConversationInfo
  snapshot: LiveSessionSnapshot | null
  deliveries: (Omit<ChatChannelMessageLog, "status"> & {
    status: ChatChannelMessageLog["status"] | "queued"
  })[]
  sampledAt: string
}

export interface ConversationCursor {
  updatedAt: string
  id: number
}

export function listChannelConversations(params: {
  channelId: number
  before?: ConversationCursor | null
  search?: string
}): Promise<ChannelConversationPage> {
  return getTransport().call("list_chat_channel_conversations", { params })
}

export function getChannelConversationRuntime(params: {
  channelId: number
  conversationId: number
}): Promise<ChannelConversationRuntime> {
  return getTransport().call("get_chat_channel_conversation_runtime", {
    params,
  })
}

export const ACTIVE_CHANNEL_PHASES = new Set([
  "preparing",
  "waiting_output",
  "replying",
  "running_tool",
  "retrying",
  "recovering",
  "background",
])
export const WAITING_CHANNEL_PHASES = new Set([
  "waiting_input",
  "waiting_confirmation",
])
