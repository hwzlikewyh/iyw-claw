"use client"

import { useCallback, useEffect, useState, type FormEvent } from "react"
import { Loader2, RefreshCw, UserRound } from "lucide-react"
import { useTranslations } from "next-intl"

import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import { useIywAccount } from "@/contexts/iyw-account-context"
import { iywAccountGetWechatQrcode, iywAccountPollWechatLogin } from "@/lib/api"
import { toErrorMessage } from "@/lib/app-error"
import { cn } from "@/lib/utils"
import type { IywWechatPollingStatus, IywWechatQrcode } from "@/lib/types"

const POLL_INTERVAL_MS = 2000
type LoginMode = "wechat" | "password"

export function AccountLoginPanel({
  active = true,
  onAuthenticated,
}: {
  active?: boolean
  onAuthenticated?: () => void
}) {
  const t = useTranslations("SidebarAccount")
  const { status, actionLoading, loginWithPassword, completeLogin } =
    useIywAccount()
  const [loginMode, setLoginMode] = useState<LoginMode>("wechat")
  const [qrcode, setQrcode] = useState<IywWechatQrcode | null>(null)
  const [qrLoading, setQrLoading] = useState(false)
  const [qrError, setQrError] = useState<string | null>(null)
  const [pollingStatus, setPollingStatus] = useState<
    IywWechatPollingStatus | "loading"
  >("loading")
  const [username, setUsername] = useState("")
  const [password, setPassword] = useState("")
  const [passwordError, setPasswordError] = useState<string | null>(null)

  const loadQrcode = useCallback(async () => {
    setQrLoading(true)
    setQrError(null)
    setQrcode(null)
    setPollingStatus("loading")
    try {
      setQrcode(await iywAccountGetWechatQrcode())
      setPollingStatus("pending")
    } catch (reason) {
      setQrError(toErrorMessage(reason))
    } finally {
      setQrLoading(false)
    }
  }, [])

  useEffect(() => {
    if (!active || loginMode !== "wechat" || status === "authenticated") return
    void loadQrcode()
  }, [active, loadQrcode, loginMode, status])

  useEffect(() => {
    if (
      !active ||
      loginMode !== "wechat" ||
      !qrcode?.qr_token ||
      status === "authenticated"
    ) {
      return
    }
    const timer = setInterval(async () => {
      try {
        const result = await iywAccountPollWechatLogin(qrcode.qr_token)
        setPollingStatus(result.status)
        if (result.status === "success" && result.profile) {
          completeLogin(result.profile)
          setQrcode(null)
          onAuthenticated?.()
        }
      } catch (reason) {
        setQrError(toErrorMessage(reason))
      }
    }, POLL_INTERVAL_MS)
    return () => clearInterval(timer)
  }, [active, completeLogin, loginMode, onAuthenticated, qrcode, status])

  const submitPassword = useCallback(
    async (event: FormEvent<HTMLFormElement>) => {
      event.preventDefault()
      setPasswordError(null)
      try {
        await loginWithPassword({ username, password })
        setUsername("")
        setPassword("")
        onAuthenticated?.()
      } catch (reason) {
        setPasswordError(toErrorMessage(reason))
      }
    },
    [loginWithPassword, onAuthenticated, password, username]
  )

  return (
    <div className="flex min-h-[20rem] flex-col gap-4 p-5">
      <div className="grid grid-cols-2 gap-1 rounded-lg bg-muted/45 p-1">
        {(["wechat", "password"] as const).map((mode) => (
          <button
            key={mode}
            type="button"
            className={cn(
              "h-8 rounded-md text-sm transition-colors",
              loginMode === mode
                ? "bg-background text-foreground shadow-sm"
                : "text-muted-foreground hover:text-foreground"
            )}
            onClick={() => {
              setLoginMode(mode)
              setQrError(null)
              setPasswordError(null)
            }}
          >
            {t(mode === "wechat" ? "wechatLoginTab" : "passwordLoginTab")}
          </button>
        ))}
      </div>

      {loginMode === "wechat" ? (
        <WechatLogin
          qrcode={qrcode}
          loading={qrLoading}
          error={qrError}
          pollingStatus={pollingStatus}
          onRefresh={loadQrcode}
        />
      ) : (
        <form
          className="mx-auto flex w-full max-w-sm flex-1 flex-col justify-center gap-4"
          onSubmit={submitPassword}
        >
          <div>
            <div className="text-base font-medium">
              {t("passwordLoginTitle")}
            </div>
            <p className="mt-1 text-sm text-muted-foreground">
              {t("passwordLoginDescription")}
            </p>
          </div>
          <div className="grid gap-2">
            <Label htmlFor="iyw-account-username">{t("username")}</Label>
            <Input
              id="iyw-account-username"
              autoComplete="username"
              placeholder={t("usernamePlaceholder")}
              value={username}
              onChange={(event) => setUsername(event.target.value)}
            />
          </div>
          <div className="grid gap-2">
            <Label htmlFor="iyw-account-password">{t("password")}</Label>
            <Input
              id="iyw-account-password"
              type="password"
              autoComplete="current-password"
              placeholder={t("passwordPlaceholder")}
              value={password}
              onChange={(event) => setPassword(event.target.value)}
            />
          </div>
          {passwordError ? <LoginError message={passwordError} /> : null}
          <Button
            type="submit"
            disabled={
              actionLoading || !username.trim() || password.length === 0
            }
          >
            {actionLoading ? (
              <Loader2 className="h-3.5 w-3.5 animate-spin" />
            ) : null}
            {actionLoading ? t("signingIn") : t("signIn")}
          </Button>
        </form>
      )}
    </div>
  )
}

function WechatLogin({
  qrcode,
  loading,
  error,
  pollingStatus,
  onRefresh,
}: {
  qrcode: IywWechatQrcode | null
  loading: boolean
  error: string | null
  pollingStatus: IywWechatPollingStatus | "loading"
  onRefresh: () => void
}) {
  const t = useTranslations("SidebarAccount")
  return (
    <div className="flex flex-1 flex-col items-center justify-center gap-4 text-center">
      <div>
        <div className="text-base font-medium">{t("wechatLoginTitle")}</div>
        <p className="mt-1 text-sm text-muted-foreground">
          {t("wechatLoginDescription")}
        </p>
      </div>
      <div className="flex h-48 w-48 items-center justify-center rounded-lg border bg-background shadow-sm">
        {loading ? (
          <Loader2 className="h-7 w-7 animate-spin text-muted-foreground" />
        ) : qrcode ? (
          // eslint-disable-next-line @next/next/no-img-element
          <img
            src={qrcode.qrcode_url}
            alt={t("wechatQrAlt")}
            className="h-44 w-44 rounded-md"
            referrerPolicy="no-referrer"
          />
        ) : (
          <UserRound className="h-8 w-8 text-muted-foreground" />
        )}
      </div>
      <div className="min-h-5 text-xs text-muted-foreground">
        {pollingStatus === "pending" ? (
          <span className="inline-flex items-center gap-1.5">
            <Loader2 className="h-3 w-3 animate-spin" />
            {t("waitingScan")}
          </span>
        ) : (
          t("scanHint")
        )}
      </div>
      {error ? <LoginError message={error} /> : null}
      <Button variant="outline" size="sm" onClick={onRefresh}>
        <RefreshCw className="h-3.5 w-3.5" />
        {t("refreshQr")}
      </Button>
    </div>
  )
}

function LoginError({ message }: { message: string }) {
  return (
    <div className="rounded-md border border-destructive/25 bg-destructive/5 px-3 py-2 text-xs text-destructive">
      {message}
    </div>
  )
}
