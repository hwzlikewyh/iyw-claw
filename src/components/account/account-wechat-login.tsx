"use client"

import { useCallback, useEffect, useRef, useState } from "react"
import { Loader2, QrCode, RefreshCw } from "lucide-react"
import { useTranslations } from "next-intl"
import { Button } from "@/components/ui/button"
import { useIywAccount } from "@/contexts/iyw-account-context"
import { iywAccountGetWechatQrcode } from "@/lib/api"
import { toErrorMessage } from "@/lib/app-error"
import { iywAccountAuth } from "@/lib/iyw-account-flow"
import type { IywWechatQrcode } from "@/lib/types"
import { AccountAgreement, AccountFlowError } from "./account-flow-feedback"

const POLL_INTERVAL_MS = 2000

export function AccountWechatLogin({
  active,
  onDone,
}: {
  active: boolean
  onDone: () => void
}) {
  const t = useTranslations("AccountFlow")
  const { qr, loading, error, setError, load } = useWechatCode(active)
  useWechatPolling({ active, qr, onDone, setError })
  return (
    <div className="grid justify-items-center gap-4 py-2 text-center">
      <div>
        <h3 className="text-base font-medium">{t("wechatTitle")}</h3>
        <p className="mt-1 text-xs text-muted-foreground">{t("wechatHint")}</p>
      </div>
      <WechatQrFrame qr={qr} loading={loading} />
      <p className="flex items-center gap-2 text-xs text-muted-foreground">
        <span className="size-1.5 rounded-full bg-primary" />
        {t("waitingScan")}
      </p>
      <AccountFlowError message={error} />
      <Button
        variant="outline"
        size="sm"
        className="h-8 rounded-lg text-xs"
        onClick={() => void load()}
        disabled={loading}
      >
        <RefreshCw className="size-3.5" />
        {t("refreshQr")}
      </Button>
      <AccountAgreement />
    </div>
  )
}
function useWechatCode(active: boolean) {
  const { invalidateLogin } = useIywAccount()
  const [qr, setQr] = useState<IywWechatQrcode | null>(null)
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState<string | null>(null)
  const generation = useRef(0)
  const load = useCallback(async () => {
    invalidateLogin()
    const current = ++generation.current
    setLoading(true)
    setError(null)
    setQr(null)
    try {
      const result = await iywAccountGetWechatQrcode()
      if (current === generation.current) setQr(result)
    } catch (reason) {
      if (current === generation.current) setError(toErrorMessage(reason))
    } finally {
      if (current === generation.current) setLoading(false)
    }
  }, [invalidateLogin])
  useEffect(() => {
    if (active) void load()
    return () => {
      generation.current += 1
      invalidateLogin()
    }
  }, [active, invalidateLogin, load])

  return { qr, loading, error, setError, load }
}
interface PollingOptions {
  active: boolean
  qr: IywWechatQrcode | null
  onDone: () => void
  setError: (error: string | null) => void
}
function useWechatPolling({ active, qr, onDone, setError }: PollingOptions) {
  const { authenticate } = useIywAccount()
  useEffect(() => {
    if (!active || !qr) return
    let cancelled = false
    let timer: ReturnType<typeof setTimeout> | undefined
    const poll = async () => {
      try {
        const result = await authenticate("wechat_poll", {
          qrToken: qr.qr_token,
        })
        if (cancelled) return
        if (result.status === "authenticated") {
          onDone()
          return
        }
        if (result.status !== "pending") return
        timer = setTimeout(() => void poll(), POLL_INTERVAL_MS)
      } catch (reason) {
        if (!cancelled) setError(toErrorMessage(reason))
      }
    }
    timer = setTimeout(() => void poll(), POLL_INTERVAL_MS)
    return () => {
      cancelled = true
      clearTimeout(timer)
      void iywAccountAuth("cancel", { qrToken: qr.qr_token }).catch(() => {})
    }
  }, [active, authenticate, onDone, qr, setError])
}
function WechatQrFrame({
  qr,
  loading,
}: {
  qr: IywWechatQrcode | null
  loading: boolean
}) {
  const t = useTranslations("AccountFlow")
  return (
    <div className="flex size-48 items-center justify-center rounded-2xl border bg-background p-3 shadow-sm">
      {loading ? (
        <Loader2 className="size-7 animate-spin text-primary/60" />
      ) : qr ? (
        // 二维码由账号服务动态签发，直接保留原始 URL。
        // eslint-disable-next-line @next/next/no-img-element
        <img
          src={qr.qrcode_url}
          alt={t("qrAlt")}
          className="size-40"
          referrerPolicy="no-referrer"
        />
      ) : (
        <QrCode className="size-8 text-muted-foreground/40" />
      )}
    </div>
  )
}
