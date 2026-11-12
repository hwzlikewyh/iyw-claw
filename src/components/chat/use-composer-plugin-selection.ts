"use client"

import { useEffect, useRef, useState } from "react"
import { useTranslations } from "next-intl"
import { toast } from "sonner"
import { useTabStore } from "@/contexts/tab-context"
import { useWorkbenchRoute } from "@/contexts/workbench-route-context"
import { getTransport } from "@/lib/transport"
import { toErrorMessage } from "@/lib/app-error"
import { prepareConnectorConversation } from "@/lib/conversation-connectors"
import { readPluginPreparation } from "@/lib/plugin-preparation"
import type { ComposerPluginChoice } from "@/lib/composer-plugins"

export function useComposerPluginSelection(input: {
  scopeKey: string
  tabId: string | null
  onSelect: (choice: ComposerPluginChoice, instruction: string) => void
}) {
  const t = useTranslations("ComposerPlugins")
  const route = useWorkbenchRoute()
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
    if (choice.blocked) {
      route.openSkillMarket(choice.slug)
      return false
    }
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
    try {
      if (choice.skills.length) {
        if (current()) {
          input.onSelect(choice, t("usePrompt", { name: choice.label }))
          return true
        }
        return false
      }
      const ready = await pluginReady(choice, {
        tabId: input.tabId,
        transport,
        current,
      })
      if (!current()) return false
      if (!ready) {
        toast.info(t("prepareRequired"))
        route.openSkillMarket(choice.slug)
        return false
      }
      input.onSelect(choice, t("usePrompt", { name: choice.label }))
      return true
    } catch (cause) {
      if (current()) setError(toErrorMessage(cause))
      return false
    } finally {
      running.current = false
      if (alive.current) setBusy(false)
    }
  }
  return { busy, error, select }
}

async function pluginReady(
  choice: ComposerPluginChoice,
  context: {
    tabId: string
    transport: ReturnType<typeof getTransport>
    current: () => boolean
  }
) {
  const tab = useTabStore
    .getState()
    .tabs.find((item) => item.id === context.tabId)
  let view = await readPluginPreparation(
    { marketSkillId: choice.id, conversationId: tab?.conversationId ?? null },
    context.transport
  )
  if (!context.current() || view.installedVersion !== choice.version)
    return false
  if (!view.connectors.length) return view.state === "unchecked"
  if (tab?.conversationId == null) {
    const prepared = await prepareConnectorConversation(
      context.tabId,
      choice.label
    )
    if (!context.current()) return false
    prepared.bind()
    view = await readPluginPreparation(
      { marketSkillId: choice.id, conversationId: prepared.id },
      context.transport
    )
  }
  return view.state === "unchecked"
}
