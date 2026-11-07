import type { pluginAppMessage, PluginAppResourceMeta } from "@/lib/api"
import type { getTransport } from "@/lib/transport"
import type { PluginDownloadHandler } from "@/lib/plugin-app-download"
import {
  PluginAppBridge,
  type PluginAppLaunch,
  type PluginAppMessage,
} from "@/lib/plugin-app-bridge"
import {
  initializeFrame,
  displayModeResult,
  openLink,
  resizeFrame,
  unsupported,
  sendToolData,
} from "./plugin-app-protocol"
type DisplayMode = "inline" | "fullscreen"
export type MessageContext = {
  message: PluginAppMessage
  launch: PluginAppLaunch
  html: string
  hostVersion: string
  resourceMeta?: PluginAppResourceMeta
  bridge: PluginAppBridge
  transport: ReturnType<typeof getTransport>
  panel: boolean
  download: PluginDownloadHandler
  currentDisplayMode: () => DisplayMode
  requestDisplayMode: (mode: DisplayMode) => DisplayMode
}

export function createBridge(input: {
  iframe: HTMLIFrameElement
  launch: PluginAppLaunch
  html: string
  hostVersion: string
  resourceMeta?: PluginAppResourceMeta
  transport: ReturnType<typeof getTransport>
  panel: boolean
  download: PluginDownloadHandler
  payload: () => Record<string, unknown>
  onInitialized: () => void
  onFailure: () => void
  currentDisplayMode: () => DisplayMode
  requestDisplayMode: (mode: DisplayMode) => DisplayMode
}): PluginAppBridge {
  let bridge: PluginAppBridge | null = null
  bridge = new PluginAppBridge(
    (message) => routeMessage({ ...input, message, bridge: bridge! }),
    () => {
      input.onInitialized()
      if (bridge) sendToolData(bridge, input.payload())
    },
    input.onFailure
  )
  return bridge
}

async function routeMessage(
  context: MessageContext & { iframe: HTMLIFrameElement }
) {
  if (context.message.method === "ui/initialize") {
    const authorized = await authorizeMessage(context)
    return authorized.error
      ? authorized
      : {
          accepted: true,
          result: initializeFrame(context),
        }
  }
  if (context.message.method === "ui/request-display-mode") {
    const authorized = await authorizeMessage(context)
    return authorized.error
      ? authorized
      : { accepted: true, result: displayModeResult(context) }
  }
  if (context.message.method === "ui/open-link") {
    return openLink(context.message, await authorizeMessage(context))
  }
  if (context.message.method === "ui/download-file") {
    const authorize = async () => {
      const result = await authorizeMessage(context, {})
      return result.accepted && !result.error
    }
    if (!(await authorize()))
      return unsupported("Plugin export is no longer authorized", -32000)
    return context.download({
      params: context.message.params,
      transport: context.transport,
      authorize,
    })
  }
  if (context.message.method === "ui/notifications/sandbox-proxy-ready") {
    const authorized = await authorizeMessage(context)
    if (authorized.error) return authorized
    return context.bridge.sendResourceReady(context.html, context.resourceMeta)
      ? { accepted: true, result: {} }
      : unsupported("Plugin app resource is too large", -32000)
  }
  if (context.message.method === "ui/notifications/request-teardown") {
    const authorized = await authorizeMessage(context)
    return authorized.error ? authorized : { accepted: true, result: {} }
  }
  if (context.message.method === "resources/read") {
    return authorizeMessage(context)
  }
  const authorized = await authorizeMessage(context)
  if (!authorized.error) resizeFrame(context.iframe, context.message)
  return authorized
}

function authorizeMessage(
  context: MessageContext,
  params: unknown = context.message.params
): ReturnType<typeof pluginAppMessage> {
  return context.transport.call("plugin_app_message", {
    request: {
      instanceId: context.launch.instanceId,
      leaseToken: context.launch.leaseToken,
      nonce: context.launch.nonce,
      method: context.message.method,
      id: context.message.id,
      params,
    },
  })
}

export function attachBridge(
  bridge: PluginAppBridge,
  iframe: HTMLIFrameElement,
  launch: PluginAppLaunch
) {
  const channel = new MessageChannel()
  iframe.contentWindow?.postMessage(
    { type: "iyw-plugin-app-init", instanceId: launch.instanceId },
    "*",
    [channel.port1]
  )
  bridge.attach(channel.port2)
}
