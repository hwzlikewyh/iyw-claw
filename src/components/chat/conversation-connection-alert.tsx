"use client"

import { useRef, useState } from "react"
import { AlertCircle, Loader2 } from "lucide-react"
import { useTranslations } from "next-intl"
import type { ConnectionStatus } from "@/lib/types"
import { toErrorMessage } from "@/lib/app-error"
import { Button } from "@/components/ui/button"

export function ConversationConnectionAlert({
  status,
  onReconnect,
}: {
  status: ConnectionStatus | null
  onReconnect?: () => Promise<void>
}) {
  const t = useTranslations("ConversationPresentation")
  const { reconnecting, error, reconnect } = useReconnect(onReconnect)
  const busy = reconnecting || status === "connecting"
  if (status !== "disconnected" && status !== "error" && !busy) return null
  return (
    <section
      role="alert"
      aria-atomic="true"
      className={`conversation-connection-alert ${busy ? "is-connecting" : ""}`}
    >
      {busy ? (
        <Loader2
          aria-hidden="true"
          className="size-5 shrink-0 animate-spin motion-reduce:animate-none"
        />
      ) : (
        <AlertCircle aria-hidden="true" className="size-5 shrink-0" />
      )}
      <div className="min-w-0 flex-1">
        <strong>{t(busy ? "reconnecting" : "disconnected")}</strong>
        <p>{t(busy ? "reconnectingHint" : "disconnectedHint")}</p>
        {error && <p className="break-words">{error}</p>}
      </div>
      {onReconnect && (
        <Button
          type="button"
          size="sm"
          variant="outline"
          disabled={busy}
          onClick={() => void reconnect()}
        >
          {t(busy ? "reconnecting" : "reconnect")}
        </Button>
      )}
    </section>
  )
}

function useReconnect(onReconnect?: () => Promise<void>) {
  const pending = useRef(false)
  const [reconnecting, setReconnecting] = useState(false)
  const [error, setError] = useState<string | null>(null)
  async function reconnect() {
    if (!onReconnect || pending.current) return
    pending.current = true
    setReconnecting(true)
    setError(null)
    try {
      await onReconnect()
    } catch (cause) {
      setError(toErrorMessage(cause))
    } finally {
      pending.current = false
      setReconnecting(false)
    }
  }
  return { reconnecting, error, reconnect }
}
