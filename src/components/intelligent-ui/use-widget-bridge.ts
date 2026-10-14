"use client"

import { useEffect, useRef, useState } from "react"
import type { UIValue } from "@/lib/intelligent-ui/engine/expression"
import { validState } from "@/lib/intelligent-ui/state-value"
import { record } from "./context"
import { widgetDocument, widgetTheme } from "./widget-document"

const MIN_HEIGHT = 96,
  MAX_HEIGHT = 640,
  READY_TIMEOUT_MS = 5000,
  READY_PING_INTERVAL_MS = 250
export function useWidgetBridge(
  input: {
    source: string
    state: Record<string, UIValue>
    update: (patch: Record<string, UIValue>) => void
  },
  frame: React.RefObject<HTMLIFrameElement | null>
) {
  const [token] = useState(() =>
    [...crypto.getRandomValues(new Uint8Array(16))]
      .map((byte) => byte.toString(16).padStart(2, "0"))
      .join("")
  )
  const [height, setHeight] = useState(MIN_HEIGHT)
  const [ready, setReady] = useState(false),
    [failed, setFailed] = useState(false)
  const [suggestion, setSuggestion] = useState("")
  const [documentHTML] = useState(() =>
    widgetDocument(input.source, token, input.state)
  )
  useWidgetMessages({
    token,
    frame,
    update: input.update,
    setHeight,
    setReady,
    setFailed,
    setSuggestion,
  })
  useEffect(() => {
    if (ready)
      frame.current?.contentWindow?.postMessage(
        { token, type: "state", state: input.state },
        "*"
      )
  }, [ready, input.state, token, frame])
  useWidgetTheme({ ready, token, frame })
  return { documentHTML, height, ready, failed, suggestion }
}
function useWidgetTheme({
  ready,
  token,
  frame,
}: {
  ready: boolean
  token: string
  frame: React.RefObject<HTMLIFrameElement | null>
}) {
  useEffect(() => {
    if (!ready) return
    const send = () =>
      frame.current?.contentWindow?.postMessage(
        { token, type: "theme", theme: widgetTheme() },
        "*"
      )
    const observer = new MutationObserver(send)
    observer.observe(document.documentElement, {
      attributes: true,
      attributeFilter: ["class", "style"],
    })
    const scheme = window.matchMedia("(prefers-color-scheme: dark)")
    scheme.addEventListener("change", send)
    send()
    return () => {
      observer.disconnect()
      scheme.removeEventListener("change", send)
    }
  }, [ready, token, frame])
}

function useWidgetMessages(input: WidgetMessages) {
  const { token, frame, setHeight, setReady, setFailed, setSuggestion } = input
  const updateRef = useLatestUpdate(input.update)
  useEffect(() => {
    const fail = () => {
      clearInterval(ping)
      clearTimeout(timer)
      setFailed(true)
    }
    const timer = setTimeout(fail, READY_TIMEOUT_MS)
    const ping = setInterval(
      () =>
        frame.current?.contentWindow?.postMessage({ token, type: "ping" }, "*"),
      READY_PING_INTERVAL_MS
    )
    const handler = (event: MessageEvent) => {
      if (event.source !== frame.current?.contentWindow) return
      const data = record(event.data)
      if (data.channel !== "iyw-widget" || data.token !== token) return
      applyWidgetMessage(data, {
        setHeight,
        setSuggestion,
        ready: () => {
          setReady(true)
          clearTimeout(timer)
          clearInterval(ping)
        },
        fail,
        update: (state) => updateRef.current(state),
      })
    }
    window.addEventListener("message", handler)
    return () => {
      clearTimeout(timer)
      clearInterval(ping)
      window.removeEventListener("message", handler)
    }
  }, [token, frame, setReady, setFailed, setHeight, setSuggestion, updateRef])
}

type WidgetMessages = {
  token: string
  frame: React.RefObject<HTMLIFrameElement | null>
  update: (state: Record<string, UIValue>) => void
  setHeight: (height: number) => void
  setReady: (ready: boolean) => void
  setFailed: (failed: boolean) => void
  setSuggestion: (text: string) => void
}

function useLatestUpdate(update: WidgetMessages["update"]) {
  const ref = useRef(update)
  useEffect(() => {
    ref.current = update
  }, [update])
  return ref
}

type WidgetDispatch = Pick<
  WidgetMessages,
  "setHeight" | "setSuggestion" | "update"
> & { ready: () => void; fail: () => void }
function applyWidgetMessage(
  data: Record<string, unknown>,
  actions: WidgetDispatch
) {
  if (data.type === "ready") actions.ready()
  if (data.type === "error") actions.fail()
  if (
    data.type === "resize" &&
    typeof data.height === "number" &&
    Number.isFinite(data.height)
  )
    actions.setHeight(Math.max(MIN_HEIGHT, Math.min(MAX_HEIGHT, data.height)))
  if (data.type === "suggest" && typeof data.text === "string")
    actions.setSuggestion(data.text.slice(0, 2000))
  if (data.type === "state") {
    if (validState(data.state)) actions.update(data.state)
    else actions.fail()
  }
}
