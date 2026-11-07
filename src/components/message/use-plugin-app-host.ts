import { useEffect, useRef, type RefObject } from "react"
import type { PluginAppHostProps } from "./plugin-app-host"
import { createBridge, attachBridge } from "./plugin-app-messaging"
import { usePluginAppLease } from "./use-plugin-app-lease"
import { usePluginAppInitialization } from "./use-plugin-app-initialization"
import { usePluginAppSize } from "./use-plugin-app-size"
import { sendToolData } from "./plugin-app-protocol"
import type { PluginAppBridge } from "@/lib/plugin-app-bridge"
import type { PluginDownloadHandler } from "@/lib/plugin-app-download"
import { registerPluginAppTeardown } from "@/lib/plugin-app-teardown"

const MAX_APP_HTML_BYTES = 8 * 1024 * 1024

export function usePluginAppHost(input: {
  props: PluginAppHostProps
  iframe: RefObject<HTMLIFrameElement | null>
  download: PluginDownloadHandler
}) {
  const { props, iframe, download } = input
  const bridge = useRef<PluginAppBridge | null>(null)
  const oversized =
    new TextEncoder().encode(props.html).byteLength > MAX_APP_HTML_BYTES
  const transport = usePluginAppLease(props.launch)
  const status = usePluginAppInitialization(props.launch.instanceId, !oversized)
  const values = useHostValues(props, bridge)
  useBridgeConnection({
    props,
    iframe,
    bridge,
    transport,
    status,
    values,
    oversized,
    download,
  })
  usePluginAppSize({
    iframe,
    bridge,
    instanceId: props.launch.instanceId,
    disabled: oversized,
  })
  return { failed: status.failed, oversized }
}

function useHostValues(
  props: PluginAppHostProps,
  bridge: RefObject<PluginAppBridge | null>
) {
  const mode = useRef({
    displayMode: props.displayMode ?? "inline",
    request: props.onDisplayModeRequest,
  })
  const payload = useRef(asRecord(props.launch.launchPayload))
  useEffect(() => {
    mode.current = {
      displayMode: props.displayMode ?? "inline",
      request: props.onDisplayModeRequest,
    }
  }, [props.displayMode, props.onDisplayModeRequest])
  useEffect(() => {
    payload.current = asRecord(props.toolUpdate ?? props.launch.launchPayload)
    if (bridge.current) sendToolData(bridge.current, payload.current)
  }, [bridge, props.toolUpdate, props.launch.launchPayload])
  useEffect(() => {
    bridge.current?.notify({
      jsonrpc: "2.0",
      method: "ui/notifications/host-context-changed",
      params: { displayMode: props.displayMode ?? "inline" },
    })
  }, [bridge, props.displayMode])
  return { mode, payload }
}

function useBridgeConnection(input: {
  props: PluginAppHostProps
  iframe: RefObject<HTMLIFrameElement | null>
  bridge: RefObject<PluginAppBridge | null>
  transport: ReturnType<typeof usePluginAppLease>
  status: ReturnType<typeof usePluginAppInitialization>
  values: ReturnType<typeof useHostValues>
  oversized: boolean
  download: PluginDownloadHandler
}) {
  const {
    props: { hostVersion, html, launch, resourceMeta, fill },
    iframe: frameRef,
    bridge: bridgeRef,
    transport,
    status: { ready, fail, failed },
    values: { mode, payload },
    oversized,
    download,
  } = input
  useEffect(() => {
    const iframe = frameRef.current
    if (!iframe || oversized) return
    const bridge = createBridge({
      iframe,
      launch,
      html,
      hostVersion,
      resourceMeta,
      transport,
      panel: !!fill,
      download,
      payload: () => payload.current,
      onInitialized: ready,
      onFailure: fail,
      currentDisplayMode: () => mode.current.displayMode,
      requestDisplayMode: (value) =>
        mode.current.request?.(value) ?? mode.current.displayMode,
    })
    return connectFrame({ bridge, iframe, launch, bridgeRef })
  }, [
    hostVersion,
    html,
    launch,
    resourceMeta,
    fill,
    frameRef,
    bridgeRef,
    transport,
    ready,
    fail,
    failed,
    mode,
    payload,
    oversized,
    download,
  ])
}

function asRecord(value: unknown): Record<string, unknown> {
  return value && typeof value === "object" && !Array.isArray(value)
    ? (value as Record<string, unknown>)
    : {}
}

function connectFrame(input: {
  bridge: PluginAppBridge
  iframe: HTMLIFrameElement
  launch: PluginAppHostProps["launch"]
  bridgeRef: RefObject<PluginAppBridge | null>
}) {
  const { bridge, iframe, launch, bridgeRef } = input
  bridgeRef.current = bridge
  const unregister = registerPluginAppTeardown(launch.instanceId, () =>
    bridge.requestTeardown()
  )
  const onLoad = () => attachBridge(bridge, iframe, launch)
  iframe.addEventListener("load", onLoad)
  return () => {
    unregister()
    iframe.removeEventListener("load", onLoad)
    bridge.dispose()
    if (bridgeRef.current === bridge) bridgeRef.current = null
  }
}
