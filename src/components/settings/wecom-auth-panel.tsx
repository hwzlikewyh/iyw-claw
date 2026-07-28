"use client"

import { useCallback, useEffect, useRef, useState } from "react"
import { Check, Copy, ExternalLink, Loader2, QrCode } from "lucide-react"
import { useTranslations } from "next-intl"

import { Button } from "@/components/ui/button"
import { wecomGetAuthStatus, wecomStartAuth } from "@/lib/api"
import { toErrorMessage } from "@/lib/app-error"

type AuthPhase = "loading" | "unauthorized" | "authorizing" | "authorized"

/**
 * WeCom (企业微信) authorization block shared by the add/edit channel
 * dialogs. Credentials are held by the wecom-cli companion: authorization is
 * a one-time QR scan, after which every WeCom channel is usable — nothing is
 * stored on the channel itself.
 */
export function WecomAuthPanel() {
  const t = useTranslations("ChatChannelSettings")
  const [phase, setPhase] = useState<AuthPhase>("loading")
  const [authUrl, setAuthUrl] = useState<string | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [copied, setCopied] = useState(false)
  const pollRef = useRef<ReturnType<typeof setInterval> | null>(null)
  // Polls tolerated after the init process exits cleanly but before
  // `authorized` flips, so a successful scan is never reported as a failure.
  const graceRef = useRef(0)

  const stopPolling = useCallback(() => {
    if (pollRef.current) {
      clearInterval(pollRef.current)
      pollRef.current = null
    }
  }, [])

  useEffect(() => {
    let disposed = false
    wecomGetAuthStatus()
      .then((status) => {
        if (disposed) return
        setPhase(status.authorized ? "authorized" : "unauthorized")
      })
      .catch((err) => {
        if (disposed) return
        setPhase("unauthorized")
        setError(toErrorMessage(err))
      })
    return () => {
      disposed = true
      stopPolling()
    }
  }, [stopPolling])

  const startAuth = useCallback(async () => {
    setError(null)
    setPhase("authorizing")
    graceRef.current = 0
    try {
      const result = await wecomStartAuth()
      setAuthUrl(result.auth_url)
      // The CLI blocks until the QR is scanned; observe completion by polling.
      stopPolling()
      pollRef.current = setInterval(() => {
        wecomGetAuthStatus()
          .then((status) => {
            if (status.authorized) {
              stopPolling()
              setPhase("authorized")
              setAuthUrl(null)
              return
            }
            // The scan is completed by the wecom-cli process, so once it is
            // gone the link is dead — surface that instead of spinning
            // forever on a pending authorization nobody is listening for.
            if (!status.auth_process_running) {
              // A clean exit can land a beat before the credential file is
              // readable, so give `authorized` one more poll to catch up
              // rather than reporting failure on a successful scan.
              if (!status.auth_process_error && graceRef.current < 1) {
                graceRef.current += 1
                return
              }
              stopPolling()
              setPhase("unauthorized")
              setAuthUrl(null)
              setError(status.auth_process_error ?? t("wecomAuthProcessGone"))
            }
          })
          .catch(() => {})
      }, 3000)
    } catch (err) {
      setPhase("unauthorized")
      setError(toErrorMessage(err))
    }
  }, [stopPolling, t])

  const copyLink = useCallback(async () => {
    if (!authUrl) return
    try {
      await navigator.clipboard.writeText(authUrl)
      setCopied(true)
      setTimeout(() => setCopied(false), 2000)
    } catch {
      // Clipboard unavailable — the link is still selectable text.
    }
  }, [authUrl])

  return (
    <div className="space-y-2 rounded-md border px-3 py-2.5">
      <div className="flex items-center justify-between gap-2">
        <span className="text-xs font-medium">{t("wecomAuthTitle")}</span>
        {phase === "loading" && (
          <Loader2 className="h-3.5 w-3.5 animate-spin text-muted-foreground" />
        )}
        {phase === "authorized" && (
          <span className="flex items-center gap-1 text-xs text-emerald-500">
            <Check className="h-3.5 w-3.5" />
            {t("wecomAuthorized")}
          </span>
        )}
        {(phase === "unauthorized" || phase === "authorizing") && (
          <Button
            size="sm"
            variant="outline"
            onClick={() => void startAuth()}
            disabled={phase === "authorizing" && !authUrl}
          >
            {phase === "authorizing" && !authUrl ? (
              <Loader2 className="mr-1 h-3.5 w-3.5 animate-spin" />
            ) : (
              <QrCode className="mr-1 h-3.5 w-3.5" />
            )}
            {t("wecomStartAuth")}
          </Button>
        )}
      </div>

      <p className="text-xs text-muted-foreground">
        {t("wecomAuthDescription")}
      </p>

      {phase === "authorizing" && authUrl && (
        <div className="space-y-1.5">
          <p className="text-xs text-muted-foreground">
            {t("wecomAuthLinkHint")}
          </p>
          <div className="flex items-center gap-1.5">
            <a
              href={authUrl}
              target="_blank"
              rel="noreferrer"
              className="inline-flex h-8 flex-1 items-center justify-center gap-1 rounded-md border bg-muted/40 px-2 text-xs font-medium text-primary underline-offset-2 hover:bg-muted/60 hover:underline"
            >
              <ExternalLink className="h-3.5 w-3.5" />
              {t("wecomOpenAuthPage")}
            </a>
            <Button
              size="icon"
              variant="ghost"
              className="h-7 w-7 shrink-0"
              onClick={() => void copyLink()}
            >
              {copied ? (
                <Check className="h-3.5 w-3.5 text-emerald-500" />
              ) : (
                <Copy className="h-3.5 w-3.5" />
              )}
            </Button>
          </div>
          <p className="flex items-center gap-1 text-xs text-muted-foreground">
            <Loader2 className="h-3 w-3 animate-spin" />
            {t("wecomWaitingScan")}
          </p>
        </div>
      )}

      {error && (
        <div className="rounded-md border border-red-500/30 bg-red-500/5 px-2 py-1.5 text-xs text-red-400">
          {error}
        </div>
      )}
    </div>
  )
}
