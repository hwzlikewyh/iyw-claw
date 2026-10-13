import { useEffect, useRef } from "react"
import { isDesktop } from "@/lib/platform"
import { getShellTransport } from "@/lib/transport"
import {
  NATIVE_PAGE_EVENT,
  type NativePageEvent,
  type NativePageState,
  type NativePageDownload,
} from "@/lib/native-page-api"
const MAX_CONSOLE_ERRORS = 100

type EventOptions = {
  props: { id: string; visible: boolean; onNewTab: (url: string) => void }
  setState: (state: NativePageState) => void
  setError: (update: (message: string | null) => string | null) => void
  loadTimeout: string
  setPicking: (value: boolean) => void
  setErrors: (update: (errors: string[]) => string[]) => void
  setFind: (query: string) => void
  sendText: (text: string) => void
  setDownloads: (
    update: (downloads: NativePageDownload[]) => NativePageDownload[]
  ) => void
}

export function useNativeEvents(options: EventOptions) {
  const latest = useRef(options)
  useEffect(() => {
    latest.current = options
  })
  useEffect(() => {
    if (!isDesktop()) return
    let active = true
    const subscribe = getShellTransport().subscribe<NativePageEvent>(
      NATIVE_PAGE_EVENT,
      (event) => {
        if (!active || event.id !== options.props.id) return
        receiveEvent(latest.current, event)
      }
    )
    return () => {
      active = false
      void subscribe.then((unsubscribe) => unsubscribe()).catch(() => {})
    }
  }, [options.props.id])
}

function receiveEvent(current: EventOptions, event: NativePageEvent) {
  if (event.type === "state" && event.state) {
    current.setState(event.state)
    if (!event.state.loading)
      current.setError((error) =>
        error === current.loadTimeout ? null : error
      )
    if (event.state.loading) {
      current.setPicking(false)
      current.setErrors(() => [])
    }
  }
  if (event.type === "error" && event.message)
    current.setErrors((errors) =>
      [...errors, event.message!].slice(-MAX_CONSOLE_ERRORS)
    )
  if (event.type === "shortcut") current.setFind("")
  if (event.type === "new-tab" && event.url && current.props.visible)
    current.props.onNewTab(event.url)
  if (event.type === "download") {
    current.setDownloads((downloads) =>
      [...downloads.filter((item) => item.url !== event.url), event].slice(
        -MAX_CONSOLE_ERRORS
      )
    )
  }
  if (event.type === "picked") {
    current.setPicking(false)
    if (!event.cancelled && current.props.visible)
      current.sendText(
        [event.url, event.tag, event.selector, event.text]
          .filter(Boolean)
          .join("\n")
      )
  }
}
