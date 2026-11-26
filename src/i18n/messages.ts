import type { AbstractIntlMessages } from "next-intl"
import enMessages from "@/i18n/messages/en.json"
import type { AppLocale } from "@/lib/types"
import { APP_NAME } from "@/lib/brand"

function replaceBrand(value: unknown): unknown {
  if (typeof value === "string") return value.replaceAll("原助理", APP_NAME)
  if (Array.isArray(value)) return value.map(replaceBrand)
  if (value && typeof value === "object") {
    return Object.fromEntries(
      Object.entries(value).map(([key, item]) => [key, replaceBrand(item)])
    )
  }
  return value
}

export function brandMessages(
  messages: AbstractIntlMessages
): AbstractIntlMessages {
  return APP_NAME === "原助理"
    ? messages
    : (replaceBrand(messages) as AbstractIntlMessages)
}

const fallbackMessages = brandMessages(enMessages)

const MESSAGE_CACHE = new Map<AppLocale, AbstractIntlMessages>([
  ["en", fallbackMessages],
])

async function loadMessages(locale: AppLocale): Promise<AbstractIntlMessages> {
  switch (locale) {
    case "zh_cn":
      return (await import("@/i18n/messages/zh-CN.json")).default
    case "en":
    default:
      return enMessages
  }
}

export function getFallbackMessages(): AbstractIntlMessages {
  return fallbackMessages
}

export async function getMessagesForLocale(
  locale: AppLocale
): Promise<AbstractIntlMessages> {
  const cached = MESSAGE_CACHE.get(locale)
  if (cached) return cached

  const messages = brandMessages(await loadMessages(locale))
  MESSAGE_CACHE.set(locale, messages)
  return messages
}
