import { describe, expect, it } from "vitest"

import { buildChatChannelConfig } from "@/lib/chat-channel-config"

describe("buildChatChannelConfig", () => {
  it("stores Telegram topic settings without credentials", () => {
    const config = buildChatChannelConfig("telegram", {
      appId: "ignored",
      baseUrl: "ignored",
      chatId: "@iyw_topics",
      topicMode: true,
    })

    expect(JSON.parse(config)).toEqual({
      chat_id: "@iyw_topics",
      topic_mode: true,
    })
    expect(config).not.toContain("token")
  })

  it("preserves existing Lark and WeChat schemas", () => {
    expect(
      JSON.parse(
        buildChatChannelConfig("lark", {
          appId: "cli_test",
          baseUrl: "ignored",
          chatId: "oc_test",
          topicMode: false,
        })
      )
    ).toEqual({ app_id: "cli_test", chat_id: "oc_test" })

    expect(
      JSON.parse(
        buildChatChannelConfig("weixin", {
          appId: "ignored",
          baseUrl: "https://ilinkai.weixin.qq.com",
          chatId: "ignored",
          topicMode: false,
        })
      )
    ).toEqual({ base_url: "https://ilinkai.weixin.qq.com" })
  })
})
