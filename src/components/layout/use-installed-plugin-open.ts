"use client"

import { useEffect, useRef, useState } from "react"
import { useTranslations } from "next-intl"
import { useTabStore } from "@/stores/tab-store"
import { useAuxPanelContext } from "@/contexts/aux-panel-context"
import { useWorkbenchRoute } from "@/contexts/workbench-route-context"
import { useBrowser } from "@/contexts/browser-context"
import { useInteractivePage } from "@/contexts/interactive-page-context"
import { preparePluginUse, type PluginUseContext } from "@/lib/plugin-use"
import { selectComposerPlugin } from "@/lib/plugin-composer-context"
import { readPluginPanels, type PluginPanelChoice } from "@/lib/plugin-panel"
import { onPluginApplicationOpen } from "@/lib/plugin-preparation-events"
import { getTransport } from "@/lib/transport"
import { toErrorMessage } from "@/lib/app-error"
import type { PluginPreparation } from "@/lib/plugin-preparation"
import type { usePluginPanelTabs } from "./use-plugin-panel-tabs"

type Tabs = ReturnType<typeof usePluginPanelTabs>

export function useInstalledPluginOpen(tabs: Tabs) {
  const t = useTranslations("PluginPreparation")
  const panel = useAuxPanelContext(),
    route = useWorkbenchRoute()
  const browser = useBrowser(),
    page = useInteractivePage()
  const state = useOpenState(tabs)
  const open = async (app: PluginPanelChoice) => {
    const tabId = useTabStore.getState().activeTabId
    if (!tabId || state.running.current) return
    const transport = getTransport()
    const current = () =>
      state.alive.current &&
      getTransport() === transport &&
      useTabStore.getState().activeTabId === tabId
    state.running.current = true
    state.setBusy(true)
    state.setError("")
    panel.openTab("home")
    route.openConversations()
    try {
      page.closePanel()
      if (browser.isOpen) await browser.closeBrowser()
      if (!current()) return
      await openPreparedApp(
        app,
        { tabId, transport, current },
        {
          open: (choice, id) => state.latest.current.open(choice, id),
          stateMessage: (value) => t(`state.${value}`),
          unavailable: t("unavailable"),
        }
      )
    } catch (cause) {
      if (current()) state.setError(toErrorMessage(cause))
      logFailure(app.pluginSlug, cause)
    } finally {
      state.running.current = false
      if (state.alive.current) state.setBusy(false)
    }
  }
  usePluginOpenRequests(open)
  return { open, busy: state.busy, error: state.error }
}

function useOpenState(tabs: Tabs) {
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
  return { busy, error, setBusy, setError, latest, alive, running }
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
  selectComposerPlugin(context.tabId, choice)
  await options.open(choice, ready.conversationId)
}

function logFailure(plugin: string, cause: unknown) {
  console.error("[plugin-use] open failed", {
    plugin,
    error: toErrorMessage(cause),
  })
}
