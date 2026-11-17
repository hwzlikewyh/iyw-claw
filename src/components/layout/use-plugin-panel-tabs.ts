import {
  useCallback,
  useEffect,
  useRef,
  useState,
  type Dispatch,
  type SetStateAction,
} from "react"
import { useAuxPanelContext } from "@/contexts/aux-panel-context"
import { useTranslations } from "next-intl"
import { useTabStore } from "@/contexts/tab-context"
import { getTransport } from "@/lib/transport"
import { readPluginPanels, type PluginPanelChoice } from "@/lib/plugin-panel"
import { openPluginPanelWindow } from "@/lib/plugin-panel-window"
import type { PluginAppOpenResponse } from "@/lib/api"
import { requestPluginAppTeardown } from "@/lib/plugin-app-teardown"
import { pluginPanelError } from "./use-plugin-panel-choices"
import {
  draftPluginTab,
  onDraftPluginContextChanged,
  persistedPluginConversation,
} from "@/lib/plugin-draft-context"
import { openPanel } from "./plugin-panel-launch"

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
interface PluginPanelOpening {
  app: PluginPanelChoice
  conversationId: number | null
  tabId: string | null
}

export function usePluginPanelTabs(
  conversationId: number | null,
  conversations: string
) {
  const t = useTranslations("PluginPanels")
  const [tabs, setTabs] = useState<PluginPanelTab[]>([])
  const [openingApp, setOpeningApp] = useState<PluginPanelOpening | null>(null)
  const [detachingId, setDetachingId] = useState<string | null>(null)
  const detaching = useRef(false)
  const [busyConversation, setBusyConversation] = useState<
      number | "standalone" | null
    >(null),
    [error, setError] = useState("")
  const [transport] = useState(getTransport)
  const lifecycle = usePanelLifecycle()
  const panel = useAuxPanelContext()
  const { setActiveTab, setOpen } = panel
  const close = usePanelClose({ panel, setTabs, setError, lifecycle, tabs })
  useEffect(() => {
    const ids = new Set(conversations.split(",").map(Number))
    const prune = () =>
      setTabs((values) =>
        values.filter(
          (tab) =>
            tab.conversationId === null ||
            ids.has(tab.conversationId) ||
            (tab.conversationId < 0 &&
              useTabStore
                .getState()
                .rawTabs.some(
                  (item) => item.id === draftPluginTab(tab.conversationId!)
                ))
        )
      )
    const timer = setTimeout(prune, 0)
    const stop = onDraftPluginContextChanged(prune)
    return () => {
      clearTimeout(timer)
      stop()
    }
  }, [conversations])
  const open = (
    app: PluginPanelChoice,
    targetConversationId = conversationId
  ) => {
    const busyTarget = targetConversationId ?? "standalone"
    const sourceTabId = useTabStore.getState().activeTabId
    const source = useTabStore
      .getState()
      .tabs.find((tab) => tab.id === sourceTabId)
    return openPanel({
      app,
      conversationId: targetConversationId,
      tabs,
      transport,
      lifecycle: lifecycle.current,
      current: () =>
        getTransport() === transport &&
        useTabStore.getState().activeTabId === sourceTabId &&
        useTabStore
          .getState()
          .tabs.some(
            (tab) =>
              tab.id === sourceTabId &&
              (!source?.workingDir || tab.workingDir === source.workingDir) &&
              tab.agentType === source?.agentType
          ),
      setOpening: (opening) =>
        setOpeningApp((current) => {
          if (opening)
            return {
              app,
              conversationId: targetConversationId,
              tabId: sourceTabId,
            }
          return current?.tabId === sourceTabId &&
            current.app.pluginSlug === app.pluginSlug &&
            current.app.appKey === app.appKey
            ? null
            : current
        }),
      startupTimedOut: () => t("startupTimedOut", { name: app.label }),
      setBusy: (value) =>
        setBusyConversation((current) =>
          value ? busyTarget : current === busyTarget ? null : current
        ),
      setError,
      commit: (tab) => {
        const state = useTabStore.getState()
        const standaloneStudio =
          tab.kind === "studio-url" && tab.conversationId === null
        const draftTabId =
          tab.conversationId != null && tab.conversationId < 0
            ? draftPluginTab(tab.conversationId)
            : undefined
        const stillOpen = standaloneStudio
          ? state.tabs.some((item) => item.id === state.activeTabId)
          : state.tabs.some((item) =>
              draftTabId
                ? item.id === draftTabId
                : item.conversationId === targetConversationId
            )
        if (!stillOpen) return false
        const activeConversationId = state.tabs.find(
          (item) => item.id === state.activeTabId
        )?.conversationId
        if (
          !standaloneStudio &&
          (draftTabId
            ? draftTabId !== state.activeTabId
            : targetConversationId !== activeConversationId)
        )
          return false
        setTabs((values) =>
          values.some((value) => value.id === tab.id)
            ? values
            : [...values, tab]
        )
        setActiveTab(`app:${tab.id}`)
        setOpen(false)
        return true
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
      const conversation = persistedPluginConversation(tab.conversationId)
      if (conversation == null) throw Error("发送消息后可将插件移到独立窗口")
      const apps = await readPluginPanels(conversation, transport)
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
      await openPluginPanelWindow(app, conversation)
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
    openingApp,
    finishOpening: (tab: PluginPanelTab) =>
      setOpeningApp((current) =>
        current?.app.pluginSlug === tab.pluginSlug &&
        current.app.appKey === tab.appKey &&
        current.conversationId === tab.conversationId
          ? null
          : current
      ),
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

function usePanelLifecycle() {
  const scope = useTabStore((state) => {
    const tab = state.tabs.find((item) => item.id === state.activeTabId)
    return `${state.activeTabId}:${tab?.agentType}`
  })
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
  }, [scope])
  return lifecycle
}
