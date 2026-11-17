import { useTabStore } from "@/stores/tab-store"
import { registerBackendScopedStoreReset } from "@/stores/backend-scoped-store-reset"
import { createChatDir } from "./api"
import { reservePreparedChatDir } from "./acp-session-preparation"
import { getTransport } from "./transport"
import type { Transport } from "./transport"

type Entry = {
  id: number
  tabId: string
  directory: string
  agent: string
  transport: Transport
  bound?: number
}
const entries = new Map<string, Entry>()
const pending = new Map<string, Promise<number>>()
const directories = new Map<string, Promise<string>>()
const listeners = new Set<() => void>()
let generation = 0

export function draftPluginContext(tabId: string) {
  const entry = entries.get(tabId)
  return entry?.transport === getTransport() ? entry : undefined
}

export function draftPluginTab(contextId: number) {
  return [...entries.values()].find(
    (entry) => entry.id === contextId && entry.transport === getTransport()
  )?.tabId
}

export function draftPluginMatches(
  contextId: number,
  conversationId: number | null
) {
  const tabId = draftPluginTab(contextId)
  return tabId ? draftPluginContext(tabId)?.bound === conversationId : false
}

export function persistedPluginConversation(contextId: number) {
  if (contextId > 0) return contextId
  const tabId = draftPluginTab(contextId)
  return tabId ? draftPluginContext(tabId)?.bound : undefined
}

export function onDraftPluginContextChanged(listener: () => void) {
  listeners.add(listener)
  return () => {
    listeners.delete(listener)
  }
}

export async function ensureDraftPluginWorkspace(
  tabId: string
): Promise<string> {
  const tab = useTabStore.getState().tabs.find((item) => item.id === tabId)
  if (!tab) throw Error("当前草稿已关闭")
  if (tab.workingDir) return tab.workingDir
  if (!tab.isChat) throw Error("请选择插件使用的项目目录")
  const existing = directories.get(tabId)
  if (existing) return existing
  const transport = getTransport()
  const request = (async () => {
    const path =
      (await reservePreparedChatDir(tab.agentType)) ??
      (await createChatDir()).path
    const current = useTabStore
      .getState()
      .tabs.find((item) => item.id === tabId)
    if (
      transport !== getTransport() ||
      !current ||
      current.agentType !== tab.agentType ||
      current.isChat !== tab.isChat ||
      current.folderId !== tab.folderId
    )
      throw Error("当前草稿已变化，请重新打开插件")
    const directory = current.workingDir || path
    useTabStore.getState().setChatDraftWorkingDir(tabId, directory)
    return directory
  })()
  directories.set(tabId, request)
  try {
    return await request
  } finally {
    directories.delete(tabId)
  }
}

export async function prepareDraftPluginContext(tabId: string) {
  const request = pending.get(tabId)
  if (request) return request
  const preparation = prepare(tabId, generation)
  pending.set(tabId, preparation)
  try {
    return await preparation
  } finally {
    if (pending.get(tabId) === preparation) pending.delete(tabId)
  }
}

async function prepare(tabId: string, expectedGeneration: number) {
  const directory = await ensureDraftPluginWorkspace(tabId)
  if (expectedGeneration !== generation)
    throw Error("插件工作区已变化，请重新打开插件")
  const tab = useTabStore.getState().tabs.find((item) => item.id === tabId)
  if (!tab) throw Error("当前草稿已关闭")
  const cached = draftPluginContext(tabId)
  if (cached?.directory === directory && cached.agent === tab.agentType)
    return cached.id
  if (cached) await release(cached)
  const transport = getTransport()
  const id = await transport.call<number>("plugin_app_draft", {
    request: {
      operation: "prepare",
      workspacePath: directory,
      agentType: tab.agentType,
    },
  })
  const entry = { id, tabId, directory, agent: tab.agentType, transport }
  if (
    expectedGeneration !== generation ||
    transport !== getTransport() ||
    !useTabStore
      .getState()
      .tabs.some(
        (item) =>
          item.id === tabId &&
          item.workingDir === directory &&
          item.agentType === tab.agentType
      )
  ) {
    await release(entry)
    throw Error("当前草稿已变化，请重新打开插件")
  }
  entries.set(tabId, entry)
  await bindDraftPluginContext(tabId)
  for (const listener of listeners) listener()
  return id
}

export async function bindDraftPluginContext(tabId: string) {
  const entry = draftPluginContext(tabId)
  const id = useTabStore
    .getState()
    .tabs.find((item) => item.id === tabId)?.conversationId
  if (!entry || id == null || entry.bound === id) return
  await entry.transport.call("plugin_app_draft", {
    request: { operation: "bind", contextId: entry.id, conversationId: id },
  })
  entry.bound = id
  for (const listener of listeners) listener()
}

async function release(entry: Entry) {
  if (entries.get(entry.tabId) === entry) {
    entries.delete(entry.tabId)
    for (const listener of listeners) listener()
  }
  await entry.transport
    .call("plugin_app_draft", {
      request: { operation: "release", contextId: entry.id },
    })
    .catch((cause) =>
      console.error("[plugin-draft] release failed", {
        contextId: entry.id,
        cause,
      })
    )
}

export function releaseDraftPluginContexts() {
  ++generation
  pending.clear()
  for (const entry of entries.values()) void release(entry)
}

export function watchDraftPluginContexts() {
  return useTabStore.subscribe((state) => {
    for (const entry of entries.values()) {
      const tab = state.rawTabs.find((item) => item.id === entry.tabId)
      if (
        !tab ||
        tab.agentType !== entry.agent ||
        tab.workingDir !== entry.directory ||
        (entry.bound != null && tab.conversationId !== entry.bound)
      )
        void release(entry)
    }
  })
}

registerBackendScopedStoreReset(releaseDraftPluginContexts)
