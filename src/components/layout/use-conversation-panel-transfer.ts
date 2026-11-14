import { useEffect, useRef } from "react"
import {
  useAuxPanelContext,
  type AuxPanelTab,
} from "@/contexts/aux-panel-context"
import {
  registerConversationPanels,
  takeConversationPanels,
  type ConversationWindowPanels,
} from "@/lib/conversation-window-panels"
import { readPluginPanels } from "@/lib/plugin-panel"
import { getTransport } from "@/lib/transport"
import { toast } from "sonner"
import { toErrorMessage } from "@/lib/app-error"
import type { usePluginPanelTabs } from "./use-plugin-panel-tabs"

export function useConversationPanelTransfer(
  tabs: ReturnType<typeof usePluginPanelTabs>,
  conversationId: number | null
) {
  const panel = useAuxPanelContext()
  const latest = useRef({ tabs, panel })
  useEffect(() => {
    latest.current = { tabs, panel }
  })
  useEffect(
    () =>
      registerConversationPanels((id) => ({
        isOpen: latest.current.panel.isOpen,
        width: latest.current.panel.width,
        activeTab: latest.current.panel.activeTab,
        apps: latest.current.tabs.tabs
          .filter((tab) => tab.conversationId === id)
          .map(({ id, pluginSlug, appKey }) => ({ id, pluginSlug, appKey })),
      })),
    []
  )
  useEffect(() => {
    if (!panel.restored || !conversationId) return
    let active = true
    const timer = setTimeout(() => {
      const incoming = takeConversationPanels()
      if (!incoming?.apps.length) return
      const transport = getTransport()
      void restorePanels(
        incoming,
        { latest, conversationId, transport },
        () => active
      ).catch((cause) => toast.error(toErrorMessage(cause)))
    }, 0)
    return () => {
      active = false
      clearTimeout(timer)
    }
  }, [conversationId, panel.restored])
}

async function restorePanels(
  incoming: ConversationWindowPanels,
  context: {
    latest: {
      current: {
        tabs: ReturnType<typeof usePluginPanelTabs>
        panel: ReturnType<typeof useAuxPanelContext>
      }
    }
    conversationId: number
    transport: ReturnType<typeof getTransport>
  },
  current: () => boolean
) {
  const choices = await readPluginPanels(
    context.conversationId,
    context.transport
  )
  const selected = incoming.apps.find(
    (app) => `app:${app.id}` === incoming.activeTab
  )
  const ordered = [
    ...incoming.apps.filter((app) => app !== selected),
    ...(selected ? [selected] : []),
  ]
  for (const previous of ordered) {
    if (!current() || getTransport() !== context.transport) return
    const app = choices.find(
      (value) =>
        value.pluginSlug === previous.pluginSlug &&
        value.appKey === previous.appKey
    )
    if (app) await context.latest.current.tabs.open(app, context.conversationId)
  }
  if (!current()) return
  if (!selected)
    context.latest.current.panel.setActiveTab(incoming.activeTab as AuxPanelTab)
  context.latest.current.panel.setWidth(incoming.width)
  context.latest.current.panel.setOpen(incoming.isOpen)
}
