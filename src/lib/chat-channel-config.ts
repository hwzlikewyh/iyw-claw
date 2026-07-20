import type { AgentType, ChannelType } from "@/lib/types"

interface ChatChannelConfigFields {
  appId: string
  baseUrl: string
  chatId: string
  /** Channel-level default agent; `null` falls through to the folder default. */
  defaultAgentType: AgentType | null
}

export function buildChatChannelConfig(
  channelType: ChannelType,
  fields: ChatChannelConfigFields
): string {
  const base: Record<string, unknown> = fields.defaultAgentType
    ? { default_agent_type: fields.defaultAgentType }
    : {}
  if (channelType === "weixin") {
    return JSON.stringify({ ...base, base_url: fields.baseUrl })
  }
  if (channelType === "wecom") {
    // Credentials live in wecom-cli (QR auth); the optional chat id only
    // targets app-initiated notifications like the daily report.
    return JSON.stringify({
      ...base,
      default_chatid: fields.chatId,
      default_chat_type: 1,
    })
  }
  return JSON.stringify({
    ...base,
    app_id: fields.appId,
    chat_id: fields.chatId,
  })
}
