import { useTabStore } from "@/stores/tab-store"
import type { PromptDraft } from "@/lib/types"

export interface PluginWindowContext {
  windowId: string
  tabId: string
  conversationId: number
  agentLabel: string
  modelId: string
  modelName: string
}

type WindowEntry = {
  context: () => Omit<PluginWindowContext, "windowId" | "tabId"> | null
  send: (draft: PromptDraft) => boolean | void | Promise<boolean | void>
}
const entries = new Map<string, WindowEntry>()
const listeners = new Set<() => void>()
let windowId: string | undefined
let detached: {
  context: () => PluginWindowContext | null
  send: (expected: PluginWindowContext, draft: PromptDraft) => Promise<void>
} | null = null

export function registerDetachedPluginContext(
  entry: NonNullable<typeof detached>
) {
  detached = entry
  notifyPluginWindowContext()
  return () => {
    if (detached === entry) detached = null
    notifyPluginWindowContext()
  }
}

export function notifyPluginWindowContext() {
  for (const listener of listeners) listener()
}

export function registerPluginWindow(tabId: string, entry: WindowEntry) {
  entries.set(tabId, entry)
  notifyPluginWindowContext()
  return () => {
    if (entries.get(tabId) !== entry) return
    entries.delete(tabId)
    notifyPluginWindowContext()
  }
}

export function readPluginWindowContext(conversationId: number) {
  if (detached) {
    const context = detached.context()
    return context?.conversationId === conversationId ? context : null
  }
  const store = useTabStore.getState()
  const tab = store.rawTabs.find((item) => item.id === store.activeTabId)
  if (!tab || tab.conversationId !== conversationId) return null
  const context = entries.get(tab.id)?.context()
  if (!context || context.conversationId !== conversationId) return null
  windowId ??= crypto.randomUUID()
  return { ...context, windowId, tabId: tab.id }
}

export async function sendPluginWindowMessage(
  expected: PluginWindowContext,
  draft: PromptDraft
) {
  const current = readPluginWindowContext(expected.conversationId)
  if (
    !current ||
    current.tabId !== expected.tabId ||
    current.windowId !== expected.windowId
  )
    throw new Error("当前原助理窗口已切换，请在目标会话重新打开插件")
  const entry = entries.get(current.tabId)
  if (detached) return detached.send(expected, draft)
  if (!entry) throw new Error("当前窗口尚未准备好接收消息")
  const accepted = await entry.send(draft)
  if (accepted === false) throw new Error("当前窗口未接受消息，请检查会话状态")
}

export function onPluginWindowContextChanged(listener: () => void) {
  listeners.add(listener)
  const unsubscribe = useTabStore.subscribe((state, previous) => {
    if (state.activeTabId !== previous.activeTabId) listener()
  })
  return () => {
    listeners.delete(listener)
    unsubscribe()
  }
}
