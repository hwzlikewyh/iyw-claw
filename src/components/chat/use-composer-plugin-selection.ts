"use client"

import { useEffect, useRef, useState } from "react"
import { useTranslations } from "next-intl"
import { useTabStore } from "@/contexts/tab-context"
import { getTransport } from "@/lib/transport"
import { toErrorMessage } from "@/lib/app-error"
import {
  preparePluginUse,
  preparePluginSkills,
  type PluginUseContext,
} from "@/lib/plugin-use"
import { readPluginPanels } from "@/lib/plugin-panel"
import { requestPluginApplicationOpen } from "@/lib/plugin-preparation-events"
import type { PluginPreparation } from "@/lib/plugin-preparation"
import type { ComposerPluginChoice } from "@/lib/composer-plugins"

export function useComposerPluginSelection(input: {
  scopeKey: string
  tabId: string | null
  onSelect: (choice: ComposerPluginChoice, instruction: string) => void
}) {
  const t = useTranslations("ComposerPlugins")
  const preparation = useTranslations("PluginPreparation")
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState("")
  const latest = useRef(input),
    alive = useRef(true),
    running = useRef(false)
  latest.current = input
  useEffect(() => {
    alive.current = true
    return () => {
      alive.current = false
    }
  }, [])
  const select = async (choice: ComposerPluginChoice) => {
    if (running.current || !input.tabId) return false
    running.current = true
    setBusy(true)
    setError("")
    const transport = getTransport(),
      scope = input.scopeKey
    const current = () =>
      alive.current &&
      latest.current.scopeKey === scope &&
      getTransport() === transport &&
      useTabStore.getState().activeTabId === input.tabId
    const context = { tabId: input.tabId, transport, current }
    try {
      const result = await prepareSelection(choice, context, (state) =>
        preparation(`state.${state}`)
      )
      if (!result || !current()) return false
      const { selected, apps } = result
      input.onSelect(selected, t("usePrompt", { name: selected.label }))
      const app = apps.find(
        (item) => item.pluginId === choice.id && item.state === "available"
      )
      if (app) requestPluginApplicationOpen(app)
      return true
    } catch (cause) {
      if (current()) setError(toErrorMessage(cause))
      console.error("[plugin-use] selection failed", {
        plugin: choice.slug,
        error: toErrorMessage(cause),
      })
      return false
    } finally {
      running.current = false
      if (alive.current) setBusy(false)
    }
  }
  return { busy, error, select }
}

async function prepareSelection(
  choice: ComposerPluginChoice,
  context: PluginUseContext,
  stateMessage: (state: PluginPreparation["state"]) => string
) {
  const ready = await preparePluginUse(choice, context)
  if (!ready || !context.current()) return null
  if (!["unchecked", "legacy"].includes(ready.view.state))
    throw Error(stateMessage(ready.view.state))
  const selected = await preparePluginSkills(choice, context)
  if (!selected || !context.current()) return null
  const apps = await readPluginPanels(ready.conversationId, context.transport)
  return { selected, apps }
}
