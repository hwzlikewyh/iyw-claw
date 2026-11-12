import { useEffect, type RefObject } from "react"
import type { PluginAppBridge } from "@/lib/plugin-app-bridge"

export function usePluginAppSize(input: {
  iframe: RefObject<HTMLIFrameElement | null>
  bridge: RefObject<PluginAppBridge | null>
  instanceId: string
  disabled: boolean
}) {
  const { iframe: frameRef, bridge, instanceId, disabled } = input
  useEffect(() => {
    const iframe = frameRef.current
    if (!iframe || disabled) return
    const observer = new ResizeObserver(() => {
      if (!iframe.clientWidth || !iframe.clientHeight) return
      bridge.current?.notify({
        jsonrpc: "2.0",
        method: "ui/notifications/host-context-changed",
        params: { containerDimensions: frameDimensions(iframe) },
      })
    })
    observer.observe(iframe)
    return () => observer.disconnect()
  }, [frameRef, bridge, instanceId, disabled])
}

export function frameDimensions(iframe: HTMLIFrameElement) {
  return {
    width: Math.max(1, iframe.clientWidth),
    height: Math.max(1, iframe.clientHeight),
  }
}
