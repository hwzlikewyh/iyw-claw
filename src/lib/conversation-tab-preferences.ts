import { toast } from "sonner"
import { getCurrentEffectiveAppLocale } from "@/lib/i18n"

export const DEFAULT_CONVERSATION_TAB_LIMIT = 8
const STORAGE_KEY = "iyw-claw:conversation-tab-limit"
const CHANGED_EVENT = "iyw-claw:conversation-tab-limit-changed"
let fallbackLimit = DEFAULT_CONVERSATION_TAB_LIMIT

export function getConversationTabLimit(): number {
  if (typeof window === "undefined") return fallbackLimit
  try {
    const raw = localStorage.getItem(STORAGE_KEY)
    const value =
      raw === null || raw.trim() === "" ? fallbackLimit : Number(raw)
    return Number.isSafeInteger(value) && value >= 0 ? value : fallbackLimit
  } catch {
    return fallbackLimit
  }
}

export function setConversationTabLimit(value: number): void {
  if (!Number.isSafeInteger(value) || value < 0) {
    throw new Error("Invalid conversation tab limit")
  }
  localStorage.setItem(STORAGE_KEY, String(value))
  fallbackLimit = value
  window.dispatchEvent(new Event(CHANGED_EVENT))
}

export function subscribeConversationTabLimit(listener: () => void) {
  const onStorage = (event: StorageEvent) => {
    if (event.key === STORAGE_KEY || event.key === null) listener()
  }
  window.addEventListener("storage", onStorage)
  window.addEventListener(CHANGED_EVENT, listener)
  return () => {
    window.removeEventListener("storage", onStorage)
    window.removeEventListener(CHANGED_EVENT, listener)
  }
}

export function conversationTabLimitMessage(): string {
  const limit = getConversationTabLimit()
  const zh = getCurrentEffectiveAppLocale() === "zh_cn"
  return zh
    ? `已达到 ${limit} 个会话标签上限，请先关闭标签，或在设置 → 通用中调整。`
    : `The ${limit} conversation tab limit is reached. Close a tab or change the limit in Settings → General.`
}

export function canOpenConversationTab(count: number): boolean {
  const limit = getConversationTabLimit()
  if (limit === 0 || count < limit) return true
  toast.info(conversationTabLimitMessage())
  return false
}
