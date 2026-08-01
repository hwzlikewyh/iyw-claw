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

/**
 * Field-level patch for `updateChatChannel` (IYW-CHANNEL-004/005). Unlike
 * `buildChatChannelConfig`, this never rebuilds the stored config: only the
 * fields the edit dialog actually owns are sent, so backend-owned fields
 * (`channel_workspace_root`, `base_url` written by QR auth, unknown fields)
 * survive an edit untouched.
 *
 * `defaultAgentType: null` is an explicit deletion (the merge treats null as
 * "remove the key"), matching the "no default agent" choice.
 */
export function buildChatChannelConfigPatch(
  channelType: ChannelType,
  fields: ChatChannelConfigFields
): string {
  const patch: Record<string, unknown> = {
    defaultAgentType: fields.defaultAgentType ?? null,
  }
  if (channelType === "lark") {
    patch.appId = fields.appId
    patch.chatId = fields.chatId
  }
  // wecom: chat id is not editable in the edit dialog (auth lives in
  // wecom-cli); weixin: base_url is written by QR auth, never here.
  return JSON.stringify(patch)
}
