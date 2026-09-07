import { useEffect, useRef, useState } from "react"
import {
  respondInteractiveHtml,
  type HtmlErrorKey,
} from "@/lib/interactive-html"
import type { HtmlCardProps } from "./interactive-html-card"

export function useHtmlCard({ page, connectionId, preview }: HtmlCardProps) {
  const [fullscreen, setFullscreen] = useState(false)
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState<HtmlErrorKey | null>(null)
  const [revision, setRevision] = useState(0)
  const response = useHtmlResponse({ page, connectionId, preview }, setError)
  useEffect(() => {
    if (!fullscreen) return
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape") setFullscreen(false)
    }
    window.addEventListener("keydown", onKey)
    return () => window.removeEventListener("keydown", onKey)
  }, [fullscreen])
  return {
    fullscreen,
    setFullscreen,
    loading,
    error,
    setError,
    revision,
    ...response,
    ready: () => {
      setLoading(false)
      setError((error) => (error === "pageError" ? error : null))
    },
    retry: () => {
      setLoading(true)
      setError(null)
      setRevision((value) => value + 1)
    },
  }
}

function useHtmlResponse(
  { page, connectionId, preview }: HtmlCardProps,
  setError: (error: HtmlErrorKey) => void
) {
  const [closed, setClosed] = useState(false)
  const [busy, setBusy] = useState(false)
  const inFlight = useRef(false)
  const waiting = page.wait_for_response && !preview && !!connectionId
  async function respond(action: "submit" | "text" | "close", data?: unknown) {
    if (inFlight.current)
      throw new Error("A response is already being submitted")
    if (preview && action === "close") {
      setClosed(true)
      return
    }
    if (!connectionId) throw new Error("The session is disconnected")
    inFlight.current = true
    setBusy(true)
    try {
      await respondInteractiveHtml(connectionId, {
        interactionId: page.interaction_id,
        action,
        data,
      })
      setClosed(true)
    } catch (error) {
      setError("submitError")
      throw error
    } finally {
      inFlight.current = false
      setBusy(false)
    }
  }

  return { closed, busy, waiting, respond }
}
export type HtmlCardState = ReturnType<typeof useHtmlCard>
