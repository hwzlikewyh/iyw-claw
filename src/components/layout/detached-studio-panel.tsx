"use client"

import { useEffect, useRef, useState } from "react"
import { Loader2 } from "lucide-react"
import { useTranslations } from "next-intl"
import { AppTitleBar } from "./app-title-bar"
import { openOpenfilmStudio, type PluginPanelChoice } from "@/lib/plugin-panel"
import { reportPluginWindowReady } from "@/lib/plugin-panel-window"
import { toErrorMessage } from "@/lib/app-error"

const PAGE_READY_TIMEOUT_MS = 60_000
type StudioInput = PluginPanelChoice & { conversationId: number }

export function DetachedStudioPanel({ input }: { input: StudioInput }) {
  const t = useTranslations("PluginPanels")
  const { url, error } = useStudioUrl(input)
  const complete = useStudioReady(
    url,
    t("startupTimedOut", { name: input.label })
  )
  return (
    <main className="fixed inset-0 flex min-h-0 flex-col bg-background text-foreground">
      <AppTitleBar
        left={
          <span data-tauri-drag-region className="truncate text-sm">
            {input.label}
          </span>
        }
      />
      <StudioContent
        url={url}
        error={error}
        label={input.label}
        complete={complete}
      />
    </main>
  )
}

function StudioContent({
  url,
  error,
  label,
  complete,
}: {
  url: string
  error: string
  label: string
  complete: (error: string | null) => void
}) {
  const t = useTranslations("PluginPanels")
  return error ? (
    <p role="alert" className="p-4 text-sm text-destructive">
      {error}
    </p>
  ) : url ? (
    <iframe
      src={url}
      title={label}
      className="min-h-0 w-full flex-1 border-0"
      sandbox="allow-scripts allow-same-origin allow-forms allow-downloads allow-modals allow-pointer-lock allow-popups"
      allow="clipboard-read; clipboard-write; fullscreen"
      referrerPolicy="no-referrer"
      onLoad={() => complete(null)}
      onError={() => complete(t("openUnavailable"))}
    />
  ) : (
    <div
      role="status"
      className="flex flex-1 items-center justify-center gap-2 text-sm text-muted-foreground"
    >
      <Loader2 className="size-4 animate-spin" aria-hidden="true" />
      {t("openingApp", { name: label })}
    </div>
  )
}

function useStudioUrl(input: StudioInput) {
  const [url, setUrl] = useState("")
  const [error, setError] = useState("")
  useEffect(() => {
    let alive = true
    const timer = setTimeout(() => {
      void openOpenfilmStudio(input.conversationId || null, input)
        .then((result) => {
          if (alive) setUrl(result.url)
        })
        .catch((cause) => {
          if (!alive) return
          const message = toErrorMessage(cause)
          setError(message)
          void reportPluginWindowReady(message).catch(console.error)
        })
    }, 0)
    return () => {
      alive = false
      clearTimeout(timer)
    }
  }, [input])
  return { url, error }
}

function useStudioReady(url: string, timeoutMessage: string) {
  const done = useRef(false)
  const timer = useRef<ReturnType<typeof setTimeout> | undefined>(undefined)
  const complete = (error: string | null) => {
    if (done.current) return
    done.current = true
    clearTimeout(timer.current)
    void reportPluginWindowReady(error).catch(console.error)
  }
  useEffect(() => {
    if (!url || done.current) return
    timer.current = setTimeout(() => {
      if (done.current) return
      done.current = true
      void reportPluginWindowReady(timeoutMessage).catch(console.error)
    }, PAGE_READY_TIMEOUT_MS)
    return () => clearTimeout(timer.current)
  }, [url, timeoutMessage])
  return complete
}
