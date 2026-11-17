import { toast } from "sonner"
import { getTransport } from "@/lib/transport"
import { draftPluginMatches } from "@/lib/plugin-draft-context"
import {
  launchPluginPanel,
  openOpenfilmStudio,
  OPENFILM_STUDIO_APP_KEY,
  type PluginPanelChoice,
} from "@/lib/plugin-panel"
import type { PluginPanelTab } from "./use-plugin-panel-tabs"
import { pluginPanelError } from "./use-plugin-panel-choices"

const MAX_TABS = 8
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
  setOpening: (opening: boolean) => void
  startupTimedOut: () => string
  setBusy: (busy: boolean) => void
  setError: (error: string) => void
  commit: (tab: PluginPanelTab) => boolean
  select: (id: string) => void
  current: () => boolean
}

export async function openPanel(context: OpeningContext) {
  const { app, conversationId, lifecycle } = context
  const isOpenfilmStudio = app.appKey === OPENFILM_STUDIO_APP_KEY
  if (
    lifecycle.runningGeneration === lifecycle.generation ||
    (!conversationId && !isOpenfilmStudio)
  )
    return
  const existing = findExistingPanel(context)
  if (existing) {
    context.select(existing.id)
    return
  }
  if (context.tabs.length >= MAX_TABS) {
    context.setError("PluginPanels.limit")
    return
  }
  await runLaunch(context)
}

function findExistingPanel(context: OpeningContext) {
  const { app, conversationId } = context
  return context.tabs.find(
    (tab) =>
      tab.pluginSlug === app.pluginSlug &&
      tab.appKey === app.appKey &&
      (tab.conversationId === conversationId ||
        (tab.conversationId != null &&
          tab.conversationId < 0 &&
          draftPluginMatches(tab.conversationId, conversationId)) ||
        (app.appKey === OPENFILM_STUDIO_APP_KEY &&
          tab.kind === "studio-url" &&
          tab.conversationId === null))
  )
}

async function runLaunch(context: OpeningContext) {
  const { app, lifecycle } = context
  lifecycle.runningGeneration = lifecycle.generation
  context.setOpening(true)
  context.setBusy(true)
  context.setError("")
  const generation = lifecycle.generation
  let committed = false
  try {
    const tab = await launchPanel(context, generation)
    if (tab) {
      committed = context.commit(tab)
      if (!committed && tab.kind === "mcp-app")
        await context.transport.call("plugin_app_teardown", {
          request: { instanceId: tab.id, conversationId: tab.conversationId },
        })
    }
  } catch (cause) {
    if (isCurrent(context, generation)) {
      const message = pluginOpenError(app, cause, context.startupTimedOut)
      context.setError(message)
      toast.error(message)
    }
  } finally {
    if (lifecycle.runningGeneration === generation) {
      lifecycle.runningGeneration = null
      context.setBusy(false)
      if (!committed) context.setOpening(false)
    }
  }
}

function isCurrent(context: OpeningContext, generation: number) {
  return (
    context.lifecycle.alive &&
    context.current() &&
    context.lifecycle.generation === generation &&
    getTransport() === context.transport
  )
}

function pluginOpenError(
  app: PluginPanelChoice,
  cause: unknown,
  startupTimedOut: () => string
) {
  const message = pluginPanelError(cause)
  return app.appKey === OPENFILM_STUDIO_APP_KEY &&
    message.toLocaleLowerCase().includes("timed out")
    ? startupTimedOut()
    : message
}

async function launchPanel(
  context: OpeningContext,
  generation: number
): Promise<PluginPanelTab | null> {
  const { app, conversationId, transport } = context
  const base = {
    label: app.label,
    pluginSlug: app.pluginSlug,
    appKey: app.appKey,
    conversationId,
  }
  if (app.appKey === OPENFILM_STUDIO_APP_KEY) {
    const { url } = await openOpenfilmStudio(conversationId, app, transport)
    if (!isCurrent(context, generation)) return null
    return {
      ...base,
      id: `studio:${conversationId ?? "standalone"}:${app.pluginSlug}`,
      kind: "studio-url",
      url,
    }
  }
  if (conversationId == null) return null
  const response = await launchPluginPanel(conversationId, app, transport)
  if (!isCurrent(context, generation)) {
    await transport.call("plugin_app_teardown", {
      request: { instanceId: response.launch.instanceId, conversationId },
    })
    return null
  }
  return {
    ...base,
    conversationId,
    id: response.launch.instanceId,
    kind: "mcp-app",
    response,
  }
}
