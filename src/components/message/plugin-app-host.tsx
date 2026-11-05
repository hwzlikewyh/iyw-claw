"use client"

import { useEffect, useRef } from "react"
import {
  pluginAppMessage,
  pluginAppTeardown,
  type PluginAppResourceMeta,
} from "@/lib/api"
import {
  initializeResult,
  displayModeResult,
  openLink,
  resizeFrame,
  unsupported,
} from "./plugin-app-protocol"
import {
  buildPluginAppProxyDocument,
  PluginAppBridge,
  type PluginAppLaunch,
  type PluginAppMessage,
} from "@/lib/plugin-app-bridge"

type DisplayMode = "inline" | "fullscreen"

type PluginAppHostProps = {
  html: string
  launch: PluginAppLaunch
  hostVersion: string
  resourceMeta?: PluginAppResourceMeta
  displayMode?: DisplayMode
  onDisplayModeRequest?: (mode: DisplayMode) => DisplayMode
}

export type MessageContext = {
  message: PluginAppMessage
  launch: PluginAppLaunch
  html: string
  hostVersion: string
  resourceMeta?: PluginAppResourceMeta
  bridge: PluginAppBridge
  currentDisplayMode: () => DisplayMode
  requestDisplayMode: (mode: DisplayMode) => DisplayMode
}

const MAX_APP_HTML_BYTES = 8 * 1024 * 1024

export function PluginAppHost({
  html,
  launch,
  hostVersion,
  resourceMeta,
  displayMode = "inline",
  onDisplayModeRequest,
}: PluginAppHostProps) {
  const iframeRef = useRef<HTMLIFrameElement>(null)
  const bridgeRef = useRef<PluginAppBridge | null>(null)
  const displayModeRef = useRef(displayMode)
  const requestModeRef = useRef(onDisplayModeRequest)
  const oversized = byteLength(html) > MAX_APP_HTML_BYTES

  useEffect(() => {
    displayModeRef.current = displayMode
    requestModeRef.current = onDisplayModeRequest
  }, [displayMode, onDisplayModeRequest])

  useEffect(() => {
    const iframe = iframeRef.current
    if (!iframe || oversized) return
    const bridge = createBridge({
      iframe,
      launch,
      html,
      hostVersion,
      resourceMeta,
      currentDisplayMode: () => displayModeRef.current,
      requestDisplayMode: (mode) =>
        requestModeRef.current?.(mode) ?? displayModeRef.current,
    })
    bridgeRef.current = bridge
    const onLoad = () => attachBridge(bridge, iframe, launch)
    iframe.addEventListener("load", onLoad)
    return () => {
      iframe.removeEventListener("load", onLoad)
      bridge.dispose()
      if (bridgeRef.current === bridge) bridgeRef.current = null
      void pluginAppTeardown({
        instanceId: launch.instanceId,
        conversationId: launch.conversationId,
      })
    }
  }, [hostVersion, html, launch, oversized, resourceMeta])

  useEffect(() => {
    bridgeRef.current?.notify({
      jsonrpc: "2.0",
      method: "ui/notifications/host-context-changed",
      params: { displayMode },
    })
  }, [displayMode])

  if (oversized) return <OversizedApp />
  return (
    <iframe
      ref={iframeRef}
      title="Plugin app"
      sandbox="allow-scripts allow-same-origin"
      src={`data:text/html;charset=utf-8,${encodeURIComponent(buildPluginAppProxyDocument())}`}
      className={
        displayMode === "fullscreen"
          ? "h-[calc(100dvh-3rem)] w-full border-0"
          : "h-[360px] w-full border-0"
      }
    />
  )
}

function createBridge(input: {
  iframe: HTMLIFrameElement
  launch: PluginAppLaunch
  html: string
  hostVersion: string
  resourceMeta?: PluginAppResourceMeta
  currentDisplayMode: () => DisplayMode
  requestDisplayMode: (mode: DisplayMode) => DisplayMode
}): PluginAppBridge {
  const payload = asRecord(input.launch.launchPayload)
  let bridge: PluginAppBridge | null = null
  bridge = new PluginAppBridge(
    (message) => routeMessage({ ...input, message, bridge: bridge! }),
    () => {
      if (bridge) sendToolData(bridge, payload)
    }
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
          result: initializeResult(
            context.currentDisplayMode(),
            context.hostVersion,
            context.resourceMeta
          ),
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

function authorizeMessage(context: MessageContext) {
  return pluginAppMessage({
    instanceId: context.launch.instanceId,
    leaseToken: context.launch.leaseToken,
    nonce: context.launch.nonce,
    method: context.message.method,
    id: context.message.id,
    params: context.message.params,
  })
}

function sendToolData(
  bridge: PluginAppBridge,
  payload: Record<string, unknown>
) {
  bridge.send({
    jsonrpc: "2.0",
    method: "ui/notifications/tool-input",
    params: { arguments: payload.arguments ?? {} },
  })
  bridge.send({
    jsonrpc: "2.0",
    method: "ui/notifications/tool-result",
    params: { result: payload.result ?? { content: [] } },
  })
}

function attachBridge(
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

function asRecord(value: unknown): Record<string, unknown> {
  return value && typeof value === "object" && !Array.isArray(value)
    ? (value as Record<string, unknown>)
    : {}
}

function byteLength(value: string) {
  return new TextEncoder().encode(value).byteLength
}

function OversizedApp() {
  return (
    <div className="border px-3 py-2 text-xs text-muted-foreground">
      Plugin app content is too large to render.
    </div>
  )
}
