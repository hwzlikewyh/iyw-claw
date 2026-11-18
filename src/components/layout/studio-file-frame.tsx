"use client"

import { useEffect, useRef, type IframeHTMLAttributes } from "react"
import { getActiveRemoteConnectionId, getTransport } from "@/lib/transport"

const FILES = "iyw.plugin.files.v1"
type Props = IframeHTMLAttributes<HTMLIFrameElement> & { src: string; enabled: boolean }

export function StudioFileFrame({ src, enabled, ...props }: Props) {
  const frame = useRef<HTMLIFrameElement>(null)
  useEffect(() => {
    const url = new URL(src)
    const element = frame.current
    if (!enabled || !element || !("__TAURI_INTERNALS__" in window) ||
        url.protocol !== "http:" || url.hostname !== "127.0.0.1" || !url.port) return
    const lifetime = new AbortController()
    const transport = getTransport()
    const current = () => !lifetime.signal.aborted && document.visibilityState === "visible" &&
      getTransport() === transport && getActiveRemoteConnectionId() == null &&
      element.isConnected && element.getClientRects().length > 0 && !element.closest("[inert]")
    const send = (message: object) => element.contentWindow?.postMessage({ channel: FILES, ...message }, url.origin)
    let picking = false
    const receive = async (event: MessageEvent) => {
      if (!current() || event.source !== element.contentWindow || event.origin !== url.origin || event.data?.channel !== FILES) return
      const { type, id, directory } = event.data
      if (type === "probe") { send({ type: "ready", proof: url.searchParams.get("key") }); return }
      if (type !== "pick" || typeof id !== "string" || id.length > 64 || document.activeElement !== element) return
      if (picking) { send({ type: "result", id, paths: [] }); return }
      picking = true
      try {
        const { open } = await import("@tauri-apps/plugin-dialog")
        if (!current()) return
        const selected = await open({ directory: directory === true, multiple: directory !== true })
        if (current()) send({ type: "result", id, paths: Array.isArray(selected) ? selected : selected ? [selected] : [] })
      } catch {
        if (current()) send({ type: "result", id, error: "系统文件选择器打开失败" })
      } finally { picking = false }
    }
    window.addEventListener("message", receive, { signal: lifetime.signal })
    void import("@tauri-apps/api/webview").then(async ({ getCurrentWebview }) => {
      if (lifetime.signal.aborted) return
      const stop = await getCurrentWebview().onDragDropEvent(({ payload }) => {
        if (payload.type !== "drop" || !current()) return
        const x = payload.position.x / window.devicePixelRatio
        const y = payload.position.y / window.devicePixelRatio
        if (document.elementFromPoint(x, y) !== element) return
        const rect = element.getBoundingClientRect()
        send({ type: "drop", paths: payload.paths, x: x - rect.left, y: y - rect.top })
      })
      if (lifetime.signal.aborted) stop()
      else lifetime.signal.addEventListener("abort", stop, { once: true })
    }).catch(() => console.warn("Studio native drop listener unavailable"))
    send({ type: "ready", proof: url.searchParams.get("key") })
    return () => {
      send({ type: "inactive" })
      lifetime.abort()
    }
  }, [src, enabled])
  return <iframe {...props} src={src} ref={frame} />
}
