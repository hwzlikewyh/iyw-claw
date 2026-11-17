import {
  useCallback,
  useEffect,
  useRef,
  useState,
  type Dispatch,
  type SetStateAction,
} from "react"
import { useAuxPanelContext } from "@/contexts/aux-panel-context"
import { toast } from "sonner"
import { useTabStore } from "@/contexts/tab-context"
import { getTransport } from "@/lib/transport"
import {
  launchPluginPanel,
  openOpenfilmStudio,
  OPENFILM_STUDIO_APP_KEY,
  readPluginPanels,
  type PluginPanelChoice,
} from "@/lib/plugin-panel"
import { openPluginPanelWindow } from "@/lib/plugin-panel-window"
import type { PluginAppOpenResponse } from "@/lib/api"
import { requestPluginAppTeardown } from "@/lib/plugin-app-teardown"
import { pluginPanelError } from "./use-plugin-panel-choices"

interface PluginPanelTabBase {
  id: string
  label: string
  pluginSlug: string
  appKey: string
  conversationId: number | null
}

export type McpPluginPanelTab = PluginPanelTabBase & {
  conversationId: number
  kind: "mcp-app"
  response: PluginAppOpenResponse
}
export type PluginPanelTab =
  | McpPluginPanelTab
  | (PluginPanelTabBase & { kind: "studio-url"; url: string })
const MAX_TABS = 8

export function usePluginPanelTabs(
  conversationId: number | null,
  conversations: string
) {
  const [tabs, setTabs] = useState<PluginPanelTab[]>([])
  const [detachingId, setDetachingId] = useState<string | null>(null)
  const detaching = useRef(false)
  const [busyConversation, setBusyConversation] = useState<
      number | "standalone" | null
    >(null),
    [error, setError] = useState("")
  const [transport] = useState(getTransport)
  const lifecycle = usePanelLifecycle(conversationId)
  const panel = useAuxPanelContext()
  const { setActiveTab, setOpen } = panel
  const close = usePanelClose({ panel, setTabs, setError, lifecycle, tabs })
  useEffect(() => {
    const ids = new Set(conversations.split(",").map(Number))
    const timer = setTimeout(
      () =>
        setTabs((values) =>
          values.filter(
            (tab) => tab.conversationId === null || ids.has(tab.conversationId)
          )
        ),
      0
    )
    return () => clearTimeout(timer)
  }, [conversations])
  const open = (
    app: PluginPanelChoice,
    targetConversationId = conversationId
  ) => {
    const busyTarget = targetConversationId ?? "standalone"
    return openPanel({
      app,
      conversationId: targetConversationId,
      tabs,
      transport,
      lifecycle: lifecycle.current,
      setBusy: (value) =>
        setBusyConversation((current) =>
          value ? busyTarget : current === busyTarget ? null : current
        ),
      setError,
      commit: (tab) => {
        const state = useTabStore.getState()
        const standaloneStudio =
          tab.kind === "studio-url" && tab.conversationId === null
        const stillOpen = standaloneStudio
          ? state.tabs.some((item) => item.id === state.activeTabId)
          : state.tabs.some(
              (item) => item.conversationId === targetConversationId
            )
        if (!stillOpen) return
        setTabs((values) =>
          values.some((value) => value.id === tab.id)
            ? values
            : [...values, tab]
        )
        const activeConversationId = state.tabs.find(
          (item) => item.id === state.activeTabId
        )?.conversationId
        if (targetConversationId !== activeConversationId) return
        setActiveTab(`app:${tab.id}`)
        setOpen(false)
      },
      select: (id) => {
        setActiveTab(`app:${id}`)
        setOpen(false)
      },
    })
  }
  const detach = async (tab: PluginPanelTab) => {
    if (tab.kind !== "mcp-app") return false
    if (detaching.current) return false
    detaching.current = true
    setDetachingId(tab.id)
    try {
      const apps = await readPluginPanels(tab.conversationId, transport)
      const app = apps.find(
        (item) =>
          item.pluginSlug === tab.pluginSlug && item.appKey === tab.appKey
      )
      if (!app || app.state !== "available")
        throw Error("插件状态已变化，请重新打开")
      if (!(await requestPluginAppTeardown(tab.id)))
        throw Error("插件尚未完成保存，原页面已保留")
      if (getTransport() !== transport)
        throw Error("工作区已变化，请重新打开插件")
      await openPluginPanelWindow(app, tab.conversationId)
      setTabs((values) => values.filter((item) => item.id !== tab.id))
      if (panel.activeTab === `app:${tab.id}`) panel.openTab("home")
      return true
    } catch (cause) {
      setError(pluginPanelError(cause))
      return false
    } finally {
      detaching.current = false
      setDetachingId(null)
    }
  }
  return {
    tabs,
    detachingId,
    busy:
      busyConversation !== null &&
      (busyConversation === "standalone"
        ? conversationId === null
        : busyConversation === conversationId),
    error,
    open,
    detach,
    close,
    transport,
  }
}

function usePanelClose(input: {
  panel: ReturnType<typeof useAuxPanelContext>
  setTabs: Dispatch<SetStateAction<PluginPanelTab[]>>
  setError: (value: string) => void
  lifecycle: ReturnType<typeof usePanelLifecycle>
  tabs: PluginPanelTab[]
}) {
  const latest = useRef(input)
  useEffect(() => {
    latest.current = input
  }, [input])
  return useCallback(async (id: string) => {
    const { lifecycle, panel, setTabs, setError } = latest.current
    const currentTab = latest.current.tabs.find((tab) => tab.id === id)
    if (!currentTab) return true
    const generation = lifecycle.current.generation
    if (currentTab.kind === "mcp-app" && !(await requestPluginAppTeardown(id)))
      return false
    if (!lifecycle.current.alive) return true
    const current = lifecycle.current.generation === generation
    setTabs((values) => values.filter((tab) => tab.id !== id))
    if (current) setError("")
    if (current && panel.activeTab === `app:${id}`) {
      panel.setActiveTab("home")
      panel.setOpen(false)
    }
    return true
  }, [])
}

function usePanelLifecycle(conversationId: number | null) {
  const lifecycle = useRef({
    alive: true,
    generation: 0,
    runningGeneration: null as number | null,
  })
  useEffect(() => {
    const active = lifecycle.current
    ++active.generation
    active.alive = true
    return () => {
      active.alive = false
    }
  }, [conversationId])
  return lifecycle
}

interface OpeningContext {
  app: PluginPanelChoice
  conversationId: number | null
  tabs: PluginPanelTab[]
  transport: ReturnType<typeof getTransport>
  lifecycle: {
    alive: boolean
    generation: number
    runningGeneration: number | null
  }
  setBusy: (busy: boolean) => void
  setError: (error: string) => void
  commit: (tab: PluginPanelTab) => void
  select: (id: string) => void
}

async function openPanel(context: OpeningContext) {
  const { app, conversationId, lifecycle } = context
  const isOpenfilmStudio = app.appKey === OPENFILM_STUDIO_APP_KEY
  if (
    lifecycle.runningGeneration === lifecycle.generation ||
    (!conversationId && !isOpenfilmStudio)
  )
    return
  const existing = context.tabs.find(
    (tab) =>
      tab.pluginSlug === app.pluginSlug &&
      tab.appKey === app.appKey &&
      (tab.conversationId === conversationId ||
        (isOpenfilmStudio &&
          tab.kind === "studio-url" &&
          tab.conversationId === null))
  )
  if (existing) {
    context.select(existing.id)
    return
  }
  if (context.tabs.length >= MAX_TABS) {
    context.setError("PluginPanels.limit")
    return
  }
  lifecycle.runningGeneration = lifecycle.generation
  context.setBusy(true)
  context.setError("")
  const generation = lifecycle.generation
  try {
    if (app.appKey === OPENFILM_STUDIO_APP_KEY) {
      const { url } = await openOpenfilmStudio(
        conversationId,
        app,
        context.transport
      )
      if (!lifecycle.alive || getTransport() !== context.transport) return
      context.commit({
        id: `studio:${conversationId ?? "standalone"}:${app.pluginSlug}`,
        label: app.label,
        pluginSlug: app.pluginSlug,
        appKey: app.appKey,
        conversationId,
        kind: "studio-url",
        url,
      })
      return
    }
    if (conversationId == null) return
    const response = await launchPluginPanel(
      conversationId,
      app,
      context.transport
    )
    if (!lifecycle.alive || lifecycle.generation !== generation) {
      await context.transport.call("plugin_app_teardown", {
        request: { instanceId: response.launch.instanceId, conversationId },
      })
      return
    }
    context.commit({
      id: response.launch.instanceId,
      label: app.label,
      pluginSlug: app.pluginSlug,
      appKey: app.appKey,
      conversationId,
      kind: "mcp-app",
      response,
    })
  } catch (cause) {
    if (lifecycle.alive && lifecycle.generation === generation) {
      context.setError(pluginPanelError(cause))
      toast.error(pluginPanelError(cause))
    }
  } finally {
    if (lifecycle.runningGeneration === generation)
      lifecycle.runningGeneration = null
    context.setBusy(false)
  }
}
