import type { ChannelType } from "@/lib/types"

interface ChatChannelConfigFields {
  appId: string
  baseUrl: string
  chatId: string
  topicMode: boolean
}

export function buildChatChannelConfig(
  channelType: ChannelType,
  fields: ChatChannelConfigFields
): string {
  if (channelType === "weixin") {
    return JSON.stringify({ base_url: fields.baseUrl })
  }
  if (channelType === "telegram") {
    return JSON.stringify({
      chat_id: fields.chatId,
      topic_mode: fields.topicMode,
    })
  }
  return JSON.stringify({
    app_id: fields.appId,
    chat_id: fields.chatId,
  })
}
