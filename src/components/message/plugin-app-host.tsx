"use client"

import { useEffect, useRef } from "react"
import {
  buildPluginAppProxyDocument,
  PluginAppBridge,
  type PluginAppLaunch,
  type PluginAppMessage,
} from "@/lib/plugin-app-bridge"

type PluginAppHostProps = {
  html: string
  launch: PluginAppLaunch
  displayMode?: "inline" | "fullscreen"
  onMessage?: (message: PluginAppMessage) => void
}

const MAX_APP_HTML_BYTES = 4 * 1024 * 1024

export function PluginAppHost({
  html,
  launch,
  displayMode = "inline",
  onMessage,
}: PluginAppHostProps) {
  const iframeRef = useRef<HTMLIFrameElement>(null)
  const oversized =
    new TextEncoder().encode(html).byteLength > MAX_APP_HTML_BYTES

  useEffect(() => {
    if (oversized) return
    const iframe = iframeRef.current
    if (!iframe) return
    const bridge = new PluginAppBridge(launch, (message) => {
      if (message.method === "ui/resize") {
        const height =
          typeof message.params === "object" && message.params
            ? Number((message.params as Record<string, unknown>).height)
            : NaN
        if (Number.isFinite(height) && height >= 120 && height <= 4096) {
          iframe.style.height = `${Math.round(height)}px`
        }
      }
      onMessage?.(message)
    })
    const onLoad = () => {
      const channel = new MessageChannel()
      iframe.contentWindow?.postMessage(
        {
          type: "iyw-plugin-app-init",
          instanceId: launch.instanceId,
          leaseToken: launch.leaseToken,
          nonce: launch.nonce,
          html,
        },
        "*",
        [channel.port1]
      )
      bridge.attach(channel.port2)
    }
    iframe.addEventListener("load", onLoad)
    return () => {
      iframe.removeEventListener("load", onLoad)
      bridge.dispose()
    }
  }, [html, launch, onMessage, oversized])

  if (oversized) {
    return (
      <div className="border px-3 py-2 text-xs text-muted-foreground">
        Plugin app content is too large to render.
      </div>
    )
  }

  return (
    <iframe
      ref={iframeRef}
      title="Plugin app"
      sandbox="allow-scripts"
      srcDoc={buildPluginAppProxyDocument()}
      className={
        displayMode === "fullscreen"
          ? "h-[min(80vh,720px)] w-full border-0"
          : "h-[360px] w-full border-0"
      }
    />
  )
}
