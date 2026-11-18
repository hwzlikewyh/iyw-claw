import { useEffect, useRef, useState } from "react"
import { useAcpEvent } from "@/contexts/acp-connections-context"
import { extractAppCommandError } from "@/lib/app-error"
import type { getTransport } from "@/lib/transport"
import type {
  PluginPanelSyncState,
  PluginPanelUpdate,
} from "@/lib/plugin-panel"
import type { PluginPanelTab } from "./use-plugin-panels"
import type { McpPluginPanelTab } from "./use-plugin-panel-tabs"

const SYNC_DELAY_MS = 40
const SYNC_INTERVAL_MS = 5_000
type States = Record<string, PluginPanelSyncState>
type Transport = ReturnType<typeof getTransport>
type Commit = (id: string, value: PluginPanelSyncState) => void
type Refresh = (instanceId?: string) => void

export function usePluginPanelUpdates(
  tabs: PluginPanelTab[],
  transport: Transport,
  activeTab: string | null
) {
  const [states, setStates] = useState<States>({})
  const schedule = useRef<Refresh>(() => {})
  useEffect(() => {
    const sync = createSyncScheduler(tabs, transport, (id, value) => {
      setStates((current) => merge(current, id, value))
    })
    schedule.current = sync.schedule
    sync.schedule()
    const stop = subscribeRefresh(transport, sync.schedule)
    return () => {
      sync.dispose()
      stop()
      schedule.current = () => {}
    }
  }, [tabs, transport])
  useEffect(() => {
    const ids = new Set(tabs.map((tab) => tab.id))
    const timer = setTimeout(
      () =>
        setStates((current) =>
          Object.fromEntries(
            Object.entries(current).filter(([id]) => ids.has(id))
          )
        ),
      0
    )
    return () => clearTimeout(timer)
  }, [tabs])
  useActivePanelRefresh(tabs, activeTab, schedule)
  useAcpEvent((event) => {
    if (event.type === "turn_complete") return schedule.current()
    if (event.type !== "tool_call" && event.type !== "tool_call_update") return
    const meta = event.meta as Record<string, unknown> | undefined
    const app = meta?.["iyw-claw.plugin-app"] as
      | Record<string, unknown>
      | undefined
    if (typeof app?.instanceId === "string") schedule.current()
  })
  return { states, refresh: () => schedule.current() }
}

function useActivePanelRefresh(
  tabs: PluginPanelTab[],
  activeTab: string | null,
  schedule: { current: Refresh }
) {
  useEffect(() => {
    const active = tabs.find(
      (tab) => tab.kind === "mcp-app" && `app:${tab.id}` === activeTab
    )
    if (!active) return
    const refresh = () => {
      if (document.visibilityState !== "hidden") schedule.current(active.id)
    }
    refresh()
    const timer = setInterval(refresh, SYNC_INTERVAL_MS)
    return () => clearInterval(timer)
  }, [activeTab, tabs, schedule])
}

function createSyncScheduler(
  tabs: PluginPanelTab[],
  transport: Transport,
  commit: Commit
) {
  let alive = true,
    running = false
  const pending = new Set<string>()
  let timer: ReturnType<typeof setTimeout> | undefined
  const blocked = new Set<string>()
  const run = async () => {
    timer = undefined
    if (!alive || running) return
    const batch = tabs.filter(
      (tab): tab is McpPluginPanelTab =>
        tab.kind === "mcp-app" && pending.has(tab.id) && !blocked.has(tab.id)
    )
    pending.clear()
    running = true
    await Promise.all(
      batch.map((tab) =>
        loadUpdate(tab, transport, (value) => {
          if (!alive) return
          if (value.forbidden) blocked.add(tab.id)
          commit(tab.id, value)
        })
      )
    )
    running = false
    if (pending.size && alive)
      timer = setTimeout(() => void run(), SYNC_DELAY_MS)
  }
  const schedule: Refresh = (instanceId) => {
    if (!alive || !tabs.length || document.visibilityState === "hidden") return
    for (const tab of tabs)
      if (tab.kind === "mcp-app" && (!instanceId || tab.id === instanceId))
        pending.add(tab.id)
    // 不重置已有计时器；连续事件不能无限推迟同步。
    if (!running && timer === undefined)
      timer = setTimeout(() => void run(), SYNC_DELAY_MS)
  }
  return {
    schedule,
    dispose: () => {
      alive = false
      clearTimeout(timer)
    },
  }
}

function subscribeRefresh(transport: Transport, refresh: () => void) {
  const visible = () => {
    if (document.visibilityState !== "hidden") refresh()
  }
  window.addEventListener("focus", visible)
  document.addEventListener("visibilitychange", visible)
  const stopReconnect = transport.onReconnect?.(refresh)
  return () => {
    window.removeEventListener("focus", visible)
    document.removeEventListener("visibilitychange", visible)
    stopReconnect?.()
  }
}

async function loadUpdate(
  tab: McpPluginPanelTab,
  transport: Transport,
  commit: (value: PluginPanelSyncState) => void
) {
  try {
    const update = await transport.call<PluginPanelUpdate | null>(
      "plugin_app_sync",
      {
        request: {
          instanceId: tab.id,
          conversationId: tab.conversationId,
          leaseToken: tab.response.launch.leaseToken,
          nonce: tab.response.launch.nonce,
        },
      }
    )
    commit(update ? { update } : {})
  } catch (cause) {
    const code = extractAppCommandError(cause)?.code
    commit({
      error: true,
      forbidden:
        code === "permission_denied" || code === "configuration_missing",
    })
  }
}

function merge(
  states: States,
  id: string,
  value: PluginPanelSyncState
): States {
  const previous = states[id]
  if (previous?.forbidden) return states
  if (
    value.update &&
    previous?.update &&
    value.update.cursor <= previous.update.cursor &&
    !previous.error
  )
    return states
  const update =
    value.update &&
    (!previous?.update || value.update.cursor > previous.update.cursor)
      ? value.update
      : previous?.update
  return { ...states, [id]: { ...value, update } }
}
