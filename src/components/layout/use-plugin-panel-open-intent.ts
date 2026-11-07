import { useEffect, useRef, useState } from "react"
import { useTabStore } from "@/contexts/tab-context"
import { useWorkbenchRoute } from "@/contexts/workbench-route-context"
import { useAuxPanelContext } from "@/contexts/aux-panel-context"
import { usePluginWorkspace } from "@/components/skills/market/use-plugin-workspace"
import { getTransport } from "@/lib/transport"
import { readPluginPanels, type PluginPanelChoice } from "@/lib/plugin-panel"
import {
  onPluginPreparation,
  onPluginApplicationOpen,
} from "@/lib/plugin-preparation-events"
import { onNewConversationOpened } from "@/stores/tab-store"
import { pluginPanelError } from "./use-plugin-panel-choices"
import { usePluginMarketNavigation } from "./use-plugin-market-navigation"

interface OpenIntent {
  tabId: string
  app: PluginPanelChoice
  conversationId: number | null
  phase: "binding" | "checking" | "preparing" | "launching"
  preparationOpened?: boolean
}
interface Options {
  conversationId: number | null
  transport: ReturnType<typeof getTransport>
  open: (app: PluginPanelChoice, isCurrent: () => boolean) => Promise<void>
}

export function usePluginPanelOpenIntent(options: Options) {
  const tabId = useTabStore((store) => store.activeTabId)
  const workspace = usePluginWorkspace(tabId)
  const route = useWorkbenchRoute()
  const navigation = usePluginMarketNavigation()
  const { isOpen, openTab } = useAuxPanelContext()
  const pending = useRef<OpenIntent | null>(null)
  const [revision, setRevision] = useState(0)
  const [error, setError] = useState("")
  const [busy, setBusy] = useState(false)
  const latest = useRef({ options, route, navigation })
  useEffect(() => {
    latest.current = { options, route, navigation }
  })
  useIntentSubscriptions({
    pending,
    setBusy,
    setRevision,
    transport: options.transport,
  })
  useHiddenIntentCancellation({ isOpen, pending, setBusy })

  const open = async (app: PluginPanelChoice) => {
    if (!tabId || pending.current) return
    const request: OpenIntent = {
      tabId,
      app,
      conversationId: options.conversationId,
      phase: "binding",
    }
    pending.current = request
    setBusy(true)
    setError("")
    const id = options.conversationId ?? (await workspace.bind())
    if (pending.current !== request) return
    if (id == null) {
      pending.current = null
      setBusy(false)
      return
    }
    request.conversationId = id
    request.phase = "checking"
    setRevision((value) => value + 1)
  }

  useAdvanceIntent({ options, pending, latest, revision, setError, setBusy })
  useApplicationOpenRequest((app) => {
    route.openConversations()
    openTab("home")
    return open(app)
  })
  return { open, busy: busy || workspace.busy, error: error || workspace.error }
}

function useApplicationOpenRequest(
  open: (app: PluginPanelChoice) => Promise<void>
) {
  const openRef = useRef(open)
  useEffect(() => {
    openRef.current = open
  })
  useEffect(
    () =>
      onPluginApplicationOpen((app) => {
        void openRef.current(app)
      }),
    []
  )
}

function useHiddenIntentCancellation({
  isOpen,
  pending: pendingRef,
  setBusy,
}: {
  isOpen: boolean
  pending: { current: OpenIntent | null }
  setBusy: (value: boolean) => void
}) {
  useEffect(() => {
    if (
      isOpen ||
      !pendingRef.current ||
      pendingRef.current.phase === "preparing"
    )
      return
    pendingRef.current = null
    const timer = setTimeout(() => {
      if (!pendingRef.current) setBusy(false)
    }, 0)
    return () => clearTimeout(timer)
  }, [isOpen, pendingRef, setBusy])
}

function useAdvanceIntent(input: {
  options: Options
  pending: { current: OpenIntent | null }
  latest: { current: Parameters<typeof advanceIntent>[1] }
  revision: number
  setError: (value: string) => void
  setBusy: (value: boolean) => void
}) {
  const { options, pending, latest, revision, setError, setBusy } = input
  useEffect(() => {
    const request = pending.current
    if (
      !request ||
      request.phase !== "checking" ||
      request.conversationId !== options.conversationId
    )
      return
    let active = true
    const valid = () =>
      active &&
      pending.current === request &&
      getTransport() === options.transport
    void advanceIntent(request, latest.current, valid)
      .catch((cause) => {
        if (valid()) setError(pluginPanelError(cause))
      })
      .finally(() => {
        if (!valid()) return
        if (request.phase !== "preparing") pending.current = null
        setBusy(false)
      })
    return () => {
      active = false
    }
  }, [
    revision,
    options.conversationId,
    options.transport,
    pending,
    latest,
    setError,
    setBusy,
  ])
}

async function advanceIntent(
  request: OpenIntent,
  context: {
    options: Options
    route: ReturnType<typeof useWorkbenchRoute>
    navigation: ReturnType<typeof usePluginMarketNavigation>
  },
  valid: () => boolean
) {
  const choices = await readPluginPanels(
    request.conversationId,
    context.options.transport
  )
  if (!valid()) return
  const app = choices.find(
    (item) =>
      item.pluginSlug === request.app.pluginSlug &&
      item.appKey === request.app.appKey
  )
  if (!app) throw Error("Plugin application is no longer available")
  request.app = app
  if (app.state !== "available") {
    request.phase = "preparing"
    if (!request.preparationOpened)
      context.navigation.openPlugin(app.pluginSlug)
    request.preparationOpened = true
    return
  }
  request.phase = "launching"
  context.route.openConversations()
  await context.options.open(app, valid)
}

function useIntentSubscriptions(input: {
  pending: { current: OpenIntent | null }
  setBusy: (value: boolean) => void
  setRevision: (update: (value: number) => number) => void
  transport: ReturnType<typeof getTransport>
}) {
  const { pending, setBusy, setRevision, transport } = input
  useEffect(() => {
    const cancel = () => {
      pending.current = null
      setBusy(false)
    }
    const stopTabs = useTabStore.subscribe((store) => {
      const request = pending.current
      if (!request) return
      const current = store.rawTabs.find((tab) => tab.id === request.tabId)
      if (
        store.activeTabId !== request.tabId ||
        (request.conversationId != null &&
          current?.conversationId !== request.conversationId)
      )
        cancel()
    })
    const stopNew = onNewConversationOpened(cancel)
    const stopReconnect = transport.onReconnect?.(cancel)
    const stopPreparation = onPluginPreparation((event) => {
      const request = pending.current
      if (
        !request ||
        request.app.pluginId !== event.pluginId ||
        request.conversationId !== event.conversationId
      )
        return
      if (event.canceled) return cancel()
      if (request.phase !== "preparing") return
      request.phase = "checking"
      setBusy(true)
      setRevision((value) => value + 1)
    })
    return () => {
      pending.current = null
      stopTabs()
      stopNew()
      stopReconnect?.()
      stopPreparation()
    }
  }, [pending, setBusy, setRevision, transport])
}
