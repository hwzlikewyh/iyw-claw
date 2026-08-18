import { saveChatChannelToken } from "@/lib/api"
import type { generateWecomAgentSecrets } from "@/lib/chat-channel-setup"

export async function saveWecomAgentSecrets(
  channelId: number,
  appSecret: string,
  secrets: ReturnType<typeof generateWecomAgentSecrets>,
  configPatchJson?: string
) {
  await saveChatChannelToken(
    channelId,
    JSON.stringify({
      version: 1,
      app_secret: appSecret.trim(),
      callback_token: secrets.callbackToken,
      encoding_aes_key: secrets.encodingAesKey,
    }),
    configPatchJson
  )
}
