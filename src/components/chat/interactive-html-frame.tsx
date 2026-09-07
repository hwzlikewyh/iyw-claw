"use client"

import { useEffect, useRef } from "react"
import {
  connectInteractiveHtml,
  interactiveHtmlDocument,
} from "@/lib/interactive-html-bridge"
import {
  HTML_LOAD_TIMEOUT_MS,
  MAX_HTML_BYTES,
  HTML_DEFAULT_HEIGHT,
  HTML_NONCE_WORDS,
  type HtmlErrorKey,
} from "@/lib/interactive-html"

type FrameProps = {
  html: string
  title: string
  waiting: boolean
  onReady: () => void
  onError: (message: HtmlErrorKey) => void
  onSubmit: (data: unknown) => Promise<void>
}

export function InteractiveHtmlFrame(props: FrameProps) {
  const container = useRef<HTMLDivElement>(null)
  const latest = useRef(props)
  useEffect(() => {
    latest.current = props
  })
  const { html, title, waiting } = props
  useEffect(() => {
    if (!container.current) return
    return mountFrame(container.current, { html, title, waiting }, latest)
  }, [html, title, waiting])
  return (
    <div
      ref={container}
      className="min-h-40 flex-1 overflow-auto [&>iframe]:w-full [&>iframe]:border-0"
    />
  )
}

function mountFrame(
  container: HTMLDivElement,
  content: Pick<FrameProps, "html" | "title" | "waiting">,
  latest: { current: FrameProps }
) {
  if (new TextEncoder().encode(content.html).byteLength > MAX_HTML_BYTES) {
    latest.current.onError("tooLarge")
    return
  }
  const frame = document.createElement("iframe")
  const nonce = Array.from(
    crypto.getRandomValues(new Uint32Array(HTML_NONCE_WORDS)),
    (value) => value.toString(16)
  ).join("-")
  frame.title = content.title
  frame.setAttribute("sandbox", "allow-scripts")
  frame.setAttribute("referrerpolicy", "no-referrer")
  frame.style.height = `${HTML_DEFAULT_HEIGHT}px`
  const timeout = window.setTimeout(
    () => latest.current.onError("loadError"),
    HTML_LOAD_TIMEOUT_MS
  )
  const dispose = connectInteractiveHtml({
    frame,
    nonce,
    waiting: content.waiting,
    onReady: () => {
      clearTimeout(timeout)
      latest.current.onReady()
    },
    onError: (message) => {
      clearTimeout(timeout)
      latest.current.onError(message)
    },
    onSubmit: (data) => latest.current.onSubmit(data),
  })
  frame.srcdoc = interactiveHtmlDocument(content.html, nonce)
  container.appendChild(frame)
  return () => {
    clearTimeout(timeout)
    dispose()
    frame.remove()
  }
}
