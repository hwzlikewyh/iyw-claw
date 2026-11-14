import {
  useCallback,
  useEffect,
  useRef,
  useState,
  type Dispatch,
  type SetStateAction,
} from "react"
import { useAuxPanelContext } from "@/contexts/aux-panel-context"
import { getTransport } from "@/lib/transport"
import {
  launchPluginPanel,
  readPluginPanels,
  type PluginPanelChoice,
} from "@/lib/plugin-panel"
import { openPluginPanelWindow } from "@/lib/plugin-panel-window"
import type { PluginAppOpenResponse } from "@/lib/api"
import { requestPluginAppTeardown } from "@/lib/plugin-app-teardown"
import { pluginPanelError } from "./use-plugin-panel-choices"

export interface PluginPanelTab {
  id: string
  label: string
  pluginSlug: string
  appKey: string
  conversationId: number
  response: PluginAppOpenResponse
}
const MAX_TABS = 8
const PANEL_MIN_WIDTH = 560
const PANEL_OPEN_WIDTH = 640

export function usePluginPanelTabs(
  conversationId: number | null,
  conversations: string
) {
  const [tabs, setTabs] = useState<PluginPanelTab[]>([])
  const [detachingId, setDetachingId] = useState<string | null>(null)
  const detaching = useRef(false)
  const [busyConversation, setBusyConversation] = useState<number | null>(null),
    [error, setError] = useState("")
  const [transport] = useState(getTransport)
  const lifecycle = usePanelLifecycle(conversationId)
  const panel = useAuxPanelContext()
  const { openTab, setWidth, width } = panel
  const close = usePanelClose({ panel, setTabs, setError, lifecycle })
  useEffect(() => {
    const ids = new Set(conversations.split(",").map(Number))
    const timer = setTimeout(
      () =>
        setTabs((values) =>
          values.filter((tab) => ids.has(tab.conversationId))
        ),
      0
    )
    return () => clearTimeout(timer)
  }, [conversations])
  const open = (
    app: PluginPanelChoice,
    targetConversationId = conversationId
  ) =>
    openPanel({
      app,
      conversationId: targetConversationId,
      tabs,
      transport,
      lifecycle: lifecycle.current,
      setBusy: (value) =>
        setBusyConversation(value ? targetConversationId : null),
      setError,
      commit: (tab) => {
        setTabs((values) => [...values, tab])
        if (width < PANEL_MIN_WIDTH) setWidth(PANEL_OPEN_WIDTH)
        openTab(`app:${tab.id}`)
      },
      select: (id) => openTab(`app:${id}`),
    })
  const detach = async (tab: PluginPanelTab) => {
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
    busy: busyConversation !== null && busyConversation === conversationId,
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
}) {
  const latest = useRef(input)
  useEffect(() => {
    latest.current = input
  }, [input])
  return useCallback(async (id: string) => {
    const lifecycle = latest.current.lifecycle.current
    const generation = lifecycle.generation
    const closed = await requestPluginAppTeardown(id)
    if (!lifecycle.alive) return true
    const current = lifecycle.generation === generation
    if (!closed) return !current
    const { panel, setTabs, setError } = latest.current
    setTabs((values) => values.filter((tab) => tab.id !== id))
    if (current) setError("")
    if (current && panel.activeTab === `app:${id}`) panel.openTab("home")
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
  if (lifecycle.runningGeneration === lifecycle.generation || !conversationId)
    return
  const existing = context.tabs.find(
    (tab) =>
      tab.conversationId === conversationId &&
      tab.pluginSlug === app.pluginSlug &&
      tab.appKey === app.appKey
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
      response,
    })
  } catch (cause) {
    if (lifecycle.alive && lifecycle.generation === generation)
      context.setError(pluginPanelError(cause))
  } finally {
    if (lifecycle.runningGeneration === generation)
      lifecycle.runningGeneration = null
    if (lifecycle.alive && lifecycle.generation === generation)
      context.setBusy(false)
  }
}
