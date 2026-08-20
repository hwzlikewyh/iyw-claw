import type { AgentType, ChatChannelInfo, ChannelType } from "@/lib/types"

export type ChannelSetupState = "pending_auth" | "pending_callback" | "ready"

export interface ChannelStoredConfig {
  setup_state?: ChannelSetupState
  callback_verified_at?: string
  corp_id?: string
  agent_id?: string
  callback_path?: string
  external_base_url?: string
  default_user_id?: string
  default_agent_type?: AgentType
  app_id?: string
  lark_region?: "feishu" | "lark"
  bot_id?: string
  client_id?: string
  chat_id?: string
  default_chatid?: string
}

export function parseChannelConfig(
  channel: ChatChannelInfo
): ChannelStoredConfig {
  try {
    const parsed: unknown = JSON.parse(channel.config_json)
    if (
      typeof parsed !== "object" ||
      parsed === null ||
      Array.isArray(parsed)
    ) {
      return {}
    }
    return parsed as ChannelStoredConfig
  } catch {
    return {}
  }
}

export function isSetupDraft(channel: ChatChannelInfo): boolean {
  const state = parseChannelConfig(channel).setup_state
  return state !== undefined && state !== "ready"
}

export function draftForType(
  channels: ChatChannelInfo[],
  channelType: ChannelType
): ChatChannelInfo | undefined {
  return channels.find(
    (channel) => channel.channel_type === channelType && isSetupDraft(channel)
  )
}

export function secureRandomString(length: number, alphabet: string): string {
  if (!globalThis.crypto?.getRandomValues) {
    throw new Error("Secure random generation is unavailable")
  }
  const limit = 256 - (256 % alphabet.length)
  let result = ""
  while (result.length < length) {
    const bytes = new Uint8Array(length - result.length + 8)
    globalThis.crypto.getRandomValues(bytes)
    for (const byte of bytes) {
      if (byte < limit) result += alphabet[byte % alphabet.length]
      if (result.length === length) break
    }
  }
  return result
}

export function generateWecomAgentSecrets() {
  const alphanumeric =
    "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789"
  const hex = "0123456789abcdef"
  return {
    callbackPath: secureRandomString(32, hex),
    callbackToken: secureRandomString(32, alphanumeric),
    encodingAesKey: secureRandomString(43, alphanumeric),
  }
}
