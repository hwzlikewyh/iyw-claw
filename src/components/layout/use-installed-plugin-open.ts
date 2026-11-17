"use client"

import { useEffect, useRef, useState } from "react"
import { useTranslations } from "next-intl"
import { useTabStore } from "@/stores/tab-store"
import { useWorkbenchRoute } from "@/contexts/workbench-route-context"
import { useAuxPanelContext } from "@/contexts/aux-panel-context"
import { useInteractivePage } from "@/contexts/interactive-page-context"
import { preparePluginUse, type PluginUseContext } from "@/lib/plugin-use"
import {
  deferComposerPlugin,
  selectComposerPlugin,
} from "@/lib/plugin-composer-context"
import {
  OPENFILM_STUDIO_APP_KEY,
  readPluginPanels,
  type PluginPanelChoice,
} from "@/lib/plugin-panel"
import { onPluginApplicationOpen } from "@/lib/plugin-preparation-events"
import { getTransport } from "@/lib/transport"
import { draftPluginContext } from "@/lib/plugin-draft-context"
import { toErrorMessage } from "@/lib/app-error"
import { toast } from "sonner"
import type { PluginPreparation } from "@/lib/plugin-preparation"
import type { usePluginPanelTabs } from "./use-plugin-panel-tabs"

type Tabs = ReturnType<typeof usePluginPanelTabs>
interface PluginOpenContext {
  tabId: string | null
  transport: ReturnType<typeof getTransport>
  current: () => boolean
  page: ReturnType<typeof useInteractivePage>
  tabs: Tabs
  stateMessage: (state: PluginPreparation["state"]) => string
  unavailable: string
}

export function useInstalledPluginOpen(tabs: Tabs) {
  const t = useTranslations("PluginPreparation")
  const route = useWorkbenchRoute()
  const { setOpen } = useAuxPanelContext()
  const page = useInteractivePage()
  const state = useOpenState(tabs)
  const open = async (app: PluginPanelChoice) => {
    const tabId = useTabStore.getState().activeTabId
    if (!tabId || state.running.current) return
    const transport = getTransport()
    const sourceTab = useTabStore
      .getState()
      .tabs.find((tab) => tab.id === tabId)
    const current = () =>
      state.alive.current &&
      getTransport() === transport &&
      useTabStore.getState().activeTabId === tabId &&
      useTabStore
        .getState()
        .tabs.some(
          (tab) =>
            tab.id === tabId &&
            tab.agentType === sourceTab?.agentType &&
            tab.isChat === sourceTab?.isChat &&
            (tab.folderId === sourceTab?.folderId ||
              (sourceTab?.isChat &&
                sourceTab.conversationId == null &&
                tab.conversationId != null &&
                tab.workingDir === draftPluginContext(tabId)?.directory)) &&
            (!sourceTab?.workingDir || tab.workingDir === sourceTab.workingDir)
        )
    state.running.current = true
    state.setBusy(true)
    state.setError("")
    state.setOpeningApp({
      app,
      tabId,
      conversationId: sourceTab?.conversationId ?? null,
    })
    route.openConversations()
    setOpen(true)
    try {
      await openPluginChoice(app, {
        tabId,
        transport,
        current,
        page,
        tabs: state.latest.current,
        stateMessage: (value) => t(`state.${value}`),
        unavailable: t("unavailable"),
      })
    } catch (cause) {
      if (current()) {
        state.setError(toErrorMessage(cause))
        toast.error(toErrorMessage(cause))
      }
      logFailure(app.pluginSlug, cause)
    } finally {
      state.running.current = false
      state.setOpeningApp(null)
      if (state.alive.current) state.setBusy(false)
    }
  }
  usePluginOpenRequests(open)
  return {
    open,
    busy: state.busy,
    error: state.error,
    openingApp: state.openingApp,
  }
}

async function openPluginChoice(
  app: PluginPanelChoice,
  context: PluginOpenContext
) {
  context.page.closePanel()
  if (!context.current()) return
  if (app.appKey === OPENFILM_STUDIO_APP_KEY) {
    const conversationId = context.tabId
      ? (useTabStore.getState().tabs.find((tab) => tab.id === context.tabId)
          ?.conversationId ?? null)
      : null
    if (context.tabId) {
      if (conversationId == null) deferComposerPlugin(context.tabId, app)
      else selectComposerPlugin(context.tabId, app)
    }
    await context.tabs.open(app, conversationId)
    return
  }
  if (!context.tabId) return
  const tab = useTabStore
    .getState()
    .tabs.find((item) => item.id === context.tabId)
  if (!tab) return
  if (tab.conversationId == null) {
    deferComposerPlugin(context.tabId, app)
  }
  await openPreparedApp(
    app,
    {
      tabId: context.tabId,
      transport: context.transport,
      current: context.current,
    },
    {
      open: (choice, id) => context.tabs.open(choice, id),
      stateMessage: context.stateMessage,
      unavailable: context.unavailable,
    }
  )
}

function useOpenState(tabs: Tabs) {
  const [openingApp, setOpeningApp] = useState<{
    app: PluginPanelChoice
    tabId: string
    conversationId: number | null
  } | null>(null)
  const [busy, setBusy] = useState(false),
    [error, setError] = useState("")
  const latest = useRef(tabs),
    alive = useRef(true),
    running = useRef(false)
  useEffect(() => {
    latest.current = tabs
  })
  useEffect(() => {
    alive.current = true
    return () => {
      alive.current = false
    }
  }, [])
  return {
    busy,
    error,
    setBusy,
    setError,
    latest,
    alive,
    running,
    openingApp,
    setOpeningApp,
  }
}

function usePluginOpenRequests(
  open: (app: PluginPanelChoice) => Promise<void>
) {
  const latest = useRef(open)
  useEffect(() => {
    latest.current = open
  })
  useEffect(
    () =>
      onPluginApplicationOpen((app) => {
        void latest.current(app)
      }),
    []
  )
}

async function openPreparedApp(
  app: PluginPanelChoice,
  context: PluginUseContext,
  options: {
    open: Tabs["open"]
    stateMessage: (state: PluginPreparation["state"]) => string
    unavailable: string
  }
) {
  const ready = await preparePluginUse(
    { id: app.pluginId, version: app.version, label: app.label },
    context
  )
  if (!ready || !context.current()) return
  if (ready.view.state !== "unchecked")
    throw Error(options.stateMessage(ready.view.state))
  const choices = await readPluginPanels(
    ready.conversationId,
    context.transport
  )
  if (!context.current()) return
  const choice = choices.find(
    (entry) =>
      entry.pluginSlug === app.pluginSlug && entry.appKey === app.appKey
  )
  if (!choice || choice.state !== "available") throw Error(options.unavailable)
  const tab = useTabStore
    .getState()
    .tabs.find((item) => item.id === context.tabId)
  if (tab?.conversationId == null) deferComposerPlugin(context.tabId, choice)
  else selectComposerPlugin(context.tabId, choice)
  await options.open(choice, ready.conversationId)
}

function logFailure(plugin: string, cause: unknown) {
  console.error("[plugin-use] open failed", {
    plugin,
    error: toErrorMessage(cause),
  })
}
