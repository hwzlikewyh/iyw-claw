"use client"

import { useEffect, useRef, useState } from "react"
import { useTranslations } from "next-intl"
import { Button } from "@/components/ui/button"
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"
import { getTransport } from "@/lib/transport"
import { openUrl } from "@/lib/platform"

interface OAuthStatus {
  id: string
  state: "starting" | "pending" | "completed" | "cancelled" | "failed"
  authorization_url: string | null
  error: string | null
}

export function ConnectorOAuthDialog({
  id,
  onClose,
}: {
  id: string
  onClose: () => void
}) {
  const t = useTranslations("ConnectorMarket")
  const [status, setStatus] = useState<OAuthStatus | null>(null)
  const [error, setError] = useState("")
  const leases = useRef(0)
  const request = useRef<{ id: string; promise: Promise<OAuthStatus> } | null>(
    null
  )
  useEffect(() => {
    leases.current += 1
    let active = true
    let timer: ReturnType<typeof setTimeout>
    const poll = async (value: OAuthStatus) => {
      if (!active) return
      setStatus(value)
      if (value.state !== "pending" && value.state !== "starting") return
      timer = setTimeout(() => {
        getTransport()
          .call<OAuthStatus>("mcp_oauth_status", { attemptId: value.id })
          .then(poll)
          .catch((cause) => {
            if (active) setError(String(cause))
          })
      }, 1000)
    }
    if (request.current?.id !== id)
      request.current = {
        id,
        promise: getTransport().call<OAuthStatus>("mcp_oauth_begin", {
          serverId: id,
        }),
      }
    const pending = request.current.promise
    pending
      .then((value) => {
        if (active) void poll(value)
      })
      .catch((cause) => {
        if (active) setError(String(cause))
      })
    return () => {
      active = false
      leases.current -= 1
      clearTimeout(timer)
      queueMicrotask(() => {
        if (leases.current === 0 || request.current?.id !== id) {
          void pending
            .then((value) =>
              getTransport().call("mcp_oauth_cancel", { attemptId: value.id })
            )
            .catch(() => {})
        }
      })
    }
  }, [id])
  return (
    <Dialog
      open
      onOpenChange={(open) => {
        if (!open) onClose()
      }}
    >
      <DialogContent>
        <DialogHeader>
          <DialogTitle>{t("authorize")}</DialogTitle>
          <DialogDescription>{t("oauthHint")}</DialogDescription>
        </DialogHeader>
        <p className="text-sm">
          {status ? t(`oauth_${status.state}`) : t("oauth_starting")}
        </p>
        {(error || status?.error) && (
          <p role="alert" className="text-sm text-destructive">
            {error || status?.error}
          </p>
        )}
        {status?.authorization_url && (
          <Button
            onClick={() => {
              void openUrl(status.authorization_url!).catch((cause) =>
                setError(String(cause))
              )
            }}
          >
            {t("openAuthorization")}
          </Button>
        )}
        <Button variant="outline" onClick={onClose}>
          {t("close")}
        </Button>
      </DialogContent>
    </Dialog>
  )
}
