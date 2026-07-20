import { describe, expect, it } from "vitest"

import enMessages from "@/i18n/messages/en.json"
import zhCnMessages from "@/i18n/messages/zh-CN.json"
import {
  APP_LOCALES,
  mapLocaleTagToAppLocale,
  normalizeLanguageSettings,
  parseLocaleFromCookieValue,
  resolveAppLocale,
} from "@/lib/i18n"

describe("two-language i18n", () => {
  it("exposes only English and Simplified Chinese", () => {
    expect(APP_LOCALES).toEqual(["en", "zh_cn"])
  })

  it("maps every Chinese locale tag to Simplified Chinese", () => {
    expect(mapLocaleTagToAppLocale("zh-CN")).toBe("zh_cn")
    expect(mapLocaleTagToAppLocale("zh-Hant")).toBe("zh_cn")
    expect(mapLocaleTagToAppLocale("zh-TW")).toBe("zh_cn")
    expect(parseLocaleFromCookieValue("zh-TW")).toBe("zh_cn")
  })

  it("rejects unsupported locale tags so system mode falls back to English", () => {
    expect(mapLocaleTagToAppLocale("ja-JP")).toBeNull()
    expect(
      resolveAppLocale({ mode: "system", language: "en" }, ["ja-JP"])
    ).toBe("en")
  })

  it("normalizes obsolete persisted language values to English", () => {
    expect(
      normalizeLanguageSettings({
        mode: "manual",
        language: "fr" as never,
      })
    ).toEqual({ mode: "manual", language: "en" })
  })

  it("requires a new conversation for user-memory policy changes", () => {
    const english = enMessages.UserMemorySettings.status.newConversationRequired
    const simplifiedChinese =
      zhCnMessages.UserMemorySettings.status.newConversationRequired

    expect(english.toLowerCase()).not.toContain("reconnect")
    expect(english).toContain("launch memory")
    expect(english).toContain("new conversation")
    expect(simplifiedChinese).not.toContain("重连")
    expect(simplifiedChinese).toContain("启动时记忆")
    expect(simplifiedChinese).toContain("新建会话")
  })
})
