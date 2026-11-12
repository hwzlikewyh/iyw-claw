import { useEffect, useRef } from "react"
import { useTabStore } from "@/contexts/tab-context"
import { getTransport, onTransportChange } from "@/lib/transport"
import { readPluginPanels } from "@/lib/plugin-panel"
import { requestPluginApplicationOpen } from "@/lib/plugin-preparation-events"
import { onNewConversationOpened } from "@/stores/tab-store"
import type { PluginPanelTab } from "./use-plugin-panel-tabs"

export function usePluginPanelRetry(input: {
  tabs: PluginPanelTab[]
  close: (id: string) => Promise<boolean>
  transport: ReturnType<typeof getTransport>
  onError: (error: string) => void
}) {
  const running = useRef(false)
  const alive = useRef(true)
  useEffect(() => {
    alive.current = true
    return () => {
      alive.current = false
    }
  }, [])
  return async (id: string) => {
    const tab = input.tabs.find((item) => item.id === id)
    if (!tab || running.current) return
    running.current = true
    const scope = watchRetryScope(tab, input.transport, alive)
    const current = scope.current
    try {
      if (!current() || !(await input.close(id))) return
      await input.transport.call("plugin_app_teardown", {
        request: { instanceId: id, conversationId: tab.conversationId },
      })
      if (!current()) return
      const catalog = await readPluginPanels(
        tab.conversationId,
        input.transport
      )
      if (!current()) return
      const app = catalog.find(
        (item) =>
          item.pluginSlug === tab.pluginSlug && item.appKey === tab.appKey
      )
      if (!app || !requestPluginApplicationOpen(app))
        throw Error("Plugin application is no longer available")
    } catch (cause) {
      if (current()) {
        input.onError(String(cause))
        throw cause
      }
    } finally {
      scope.dispose()
      running.current = false
    }
  }
}

function watchRetryScope(
  tab: PluginPanelTab,
  transport: ReturnType<typeof getTransport>,
  alive: { current: boolean }
) {
  const tabId = useTabStore.getState().activeTabId
  let canceled = false
  const current = () => {
    const store = useTabStore.getState()
    return (
      !canceled &&
      alive.current &&
      getTransport() === transport &&
      store.activeTabId === tabId &&
      store.tabs.some(
        (item) =>
          item.id === tabId && item.conversationId === tab.conversationId
      )
    )
  }
  const cancel = () => {
    canceled = true
  }
  const stopTabs = useTabStore.subscribe(() => {
    if (!current()) cancel()
  })
  const stopNew = onNewConversationOpened(cancel)
  const stopTransport = onTransportChange(cancel)
  const stopReconnect = transport.onReconnect?.(cancel)
  return {
    current,
    dispose: () => {
      stopTabs()
      stopNew()
      stopTransport()
      stopReconnect?.()
    },
  }
}
