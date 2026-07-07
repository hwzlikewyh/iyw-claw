"use client"

import {
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
  type FormEvent,
} from "react"
import { Loader2, LogOut, RefreshCw, Settings, UserRound } from "lucide-react"
import { useTranslations } from "next-intl"

import { Avatar, AvatarFallback, AvatarImage } from "@/components/ui/avatar"
import { Button } from "@/components/ui/button"
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"
import {
  iywAccountGetProfile,
  iywAccountGetWechatQrcode,
  iywAccountLoginWithPassword,
  iywAccountLogout,
  iywAccountPollWechatLogin,
} from "@/lib/api"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import { toErrorMessage } from "@/lib/app-error"
import { cn } from "@/lib/utils"
import type {
  IywAccountProfile,
  IywWechatPollingStatus,
  IywWechatQrcode,
} from "@/lib/types"

const POLL_INTERVAL_MS = 2000
const DEFAULT_AVATAR_URL =
  "https://chdesign.oss-cn-shanghai.aliyuncs.com/static/avatar/default.png?x-oss-process=image/resize,m_mfit,h_35"

type LoginMode = "wechat" | "password"

function displayName(profile: IywAccountProfile | null, fallback: string) {
  if (!profile?.logged_in) return fallback
  return (
    profile.nick_name?.trim() ||
    profile.name?.trim() ||
    profile.phone?.trim() ||
    fallback
  )
}

function accountSubtitle(profile: IywAccountProfile | null, fallback: string) {
  if (!profile?.logged_in) return fallback
  return profile.org_name?.trim() || profile.phone?.trim() || fallback
}

function balancePoints(profile: IywAccountProfile | null, fallback: string) {
  return profile?.balance_points === null ||
    profile?.balance_points === undefined
    ? fallback
    : String(profile.balance_points)
}

function avatarFallback(profile: IywAccountProfile | null) {
  const name = displayName(profile, "")
  return name.trim().slice(0, 1).toUpperCase() || "I"
}

function AccountAvatar({
  profile,
  className,
}: {
  profile: IywAccountProfile | null
  className?: string
}) {
  const avatarUrl =
    profile?.avatar_url?.trim() ||
    (profile?.logged_in ? DEFAULT_AVATAR_URL : "")

  return (
    <Avatar className={className}>
      {avatarUrl ? (
        <AvatarImage
          src={avatarUrl}
          alt={displayName(profile, "iyw")}
          referrerPolicy="no-referrer"
        />
      ) : null}
      <AvatarFallback>{avatarFallback(profile)}</AvatarFallback>
    </Avatar>
  )
}

function AccountLoginPanel({
  loginMode,
  qrcode,
  loading,
  error,
  pollingStatus,
  onRefresh,
  onLoginModeChange,
  username,
  password,
  passwordLoading,
  passwordError,
  onUsernameChange,
  onPasswordChange,
  onPasswordSubmit,
}: {
  loginMode: LoginMode
  qrcode: IywWechatQrcode | null
  loading: boolean
  error: string | null
  pollingStatus: IywWechatPollingStatus | "loading"
  onRefresh: () => void
  onLoginModeChange: (mode: LoginMode) => void
  username: string
  password: string
  passwordLoading: boolean
  passwordError: string | null
  onUsernameChange: (value: string) => void
  onPasswordChange: (value: string) => void
  onPasswordSubmit: (event: FormEvent<HTMLFormElement>) => void
}) {
  const t = useTranslations("SidebarAccount")
  const passwordSubmitDisabled =
    passwordLoading || username.trim().length === 0 || password.length === 0

  return (
    <div className="flex min-h-[20rem] flex-col gap-4 p-5">
      <div className="grid grid-cols-2 gap-1 rounded-lg bg-muted/45 p-1">
        <button
          type="button"
          className={cn(
            "h-8 rounded-md text-sm transition-colors",
            loginMode === "wechat"
              ? "bg-background text-foreground shadow-sm"
              : "text-muted-foreground hover:text-foreground"
          )}
          onClick={() => onLoginModeChange("wechat")}
        >
          {t("wechatLoginTab")}
        </button>
        <button
          type="button"
          className={cn(
            "h-8 rounded-md text-sm transition-colors",
            loginMode === "password"
              ? "bg-background text-foreground shadow-sm"
              : "text-muted-foreground hover:text-foreground"
          )}
          onClick={() => onLoginModeChange("password")}
        >
          {t("passwordLoginTab")}
        </button>
      </div>

      {loginMode === "wechat" ? (
        <div className="flex flex-1 flex-col items-center justify-center gap-4 text-center">
          <div>
            <div className="text-base font-medium">{t("wechatLoginTitle")}</div>
            <p className="mt-1 text-sm text-muted-foreground">
              {t("wechatLoginDescription")}
            </p>
          </div>

          <div className="flex h-48 w-48 items-center justify-center rounded-lg border border-border bg-background shadow-sm">
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
                <Loader2 className="h-3 w-3 animate-spin" aria-hidden="true" />
                {t("waitingScan")}
              </span>
            ) : (
              t("scanHint")
            )}
          </div>

          {error ? (
            <div className="rounded-md border border-destructive/25 bg-destructive/5 px-3 py-2 text-xs text-destructive">
              {error}
            </div>
          ) : null}

          <Button variant="outline" size="sm" onClick={onRefresh}>
            <RefreshCw className="h-3.5 w-3.5" aria-hidden="true" />
            {t("refreshQr")}
          </Button>
        </div>
      ) : (
        <form
          className="mx-auto flex w-full max-w-sm flex-1 flex-col justify-center gap-4"
          onSubmit={onPasswordSubmit}
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
              onChange={(event) => onUsernameChange(event.target.value)}
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
              onChange={(event) => onPasswordChange(event.target.value)}
            />
          </div>

          {passwordError ? (
            <div className="rounded-md border border-destructive/25 bg-destructive/5 px-3 py-2 text-xs text-destructive">
              {passwordError}
            </div>
          ) : null}

          <Button type="submit" disabled={passwordSubmitDisabled}>
            {passwordLoading ? (
              <Loader2 className="h-3.5 w-3.5 animate-spin" aria-hidden />
            ) : null}
            {passwordLoading ? t("signingIn") : t("signIn")}
          </Button>
        </form>
      )}
    </div>
  )
}

function AccountProfilePanel({
  profile,
  loading,
  onLogout,
}: {
  profile: IywAccountProfile
  loading: boolean
  onLogout: () => void
}) {
  const t = useTranslations("SidebarAccount")
  const balance = t("balanceValue", {
    points: balancePoints(profile, t("balanceUnknown")),
  })

  return (
    <div className="flex min-h-[20rem] flex-col gap-4 p-5">
      <div className="flex items-center gap-3">
        <AccountAvatar profile={profile} className="h-11 w-11" />
        <div className="min-w-0 flex-1">
          <div className="truncate text-base font-medium">
            {displayName(profile, t("signedIn"))}
          </div>
          <div className="mt-0.5 truncate text-sm text-muted-foreground">
            {accountSubtitle(profile, t("accountReady"))}
          </div>
        </div>
      </div>

      <div className="grid gap-2 rounded-lg border border-border bg-muted/25 p-3">
        <div className="flex items-center justify-between gap-3">
          <span className="text-sm text-muted-foreground">
            {t("balancePoints")}
          </span>
          <span className="font-mono text-sm font-medium">{balance}</span>
        </div>
        {profile.balance_expiry_time ? (
          <div className="flex items-center justify-between gap-3">
            <span className="text-sm text-muted-foreground">
              {t("balanceExpiry")}
            </span>
            <span className="text-sm">{profile.balance_expiry_time}</span>
          </div>
        ) : null}
      </div>

      <div className="grid gap-2 rounded-lg border border-border bg-muted/25 p-3">
        <InfoRow label={t("userId")} value={profile.user_id} />
        <InfoRow label={t("phone")} value={profile.phone} />
        <InfoRow label={t("organization")} value={profile.org_name} />
      </div>

      <div className="mt-auto flex justify-end">
        <Button
          variant="outline"
          size="sm"
          onClick={onLogout}
          disabled={loading}
        >
          {loading ? (
            <Loader2 className="h-3.5 w-3.5 animate-spin" aria-hidden="true" />
          ) : (
            <LogOut className="h-3.5 w-3.5" aria-hidden="true" />
          )}
          {t("logout")}
        </Button>
      </div>
    </div>
  )
}

function InfoRow({
  label,
  value,
}: {
  label: string
  value: string | null | undefined
}) {
  const t = useTranslations("SidebarAccount")
  return (
    <div className="flex items-center justify-between gap-3">
      <span className="text-sm text-muted-foreground">{label}</span>
      <span className="min-w-0 truncate text-right text-sm">
        {value?.trim() || t("emptyValue")}
      </span>
    </div>
  )
}

export function SidebarAccountSettings() {
  const t = useTranslations("SidebarAccount")
  const [open, setOpen] = useState(false)
  const [loginMode, setLoginMode] = useState<LoginMode>("wechat")
  const [profile, setProfile] = useState<IywAccountProfile | null>(null)
  const [profileLoading, setProfileLoading] = useState(false)
  const [actionLoading, setActionLoading] = useState(false)
  const [qrcode, setQrcode] = useState<IywWechatQrcode | null>(null)
  const [qrLoading, setQrLoading] = useState(false)
  const [qrError, setQrError] = useState<string | null>(null)
  const [username, setUsername] = useState("")
  const [password, setPassword] = useState("")
  const [passwordLoading, setPasswordLoading] = useState(false)
  const [passwordError, setPasswordError] = useState<string | null>(null)
  const [pollingStatus, setPollingStatus] = useState<
    IywWechatPollingStatus | "loading"
  >("loading")
  const pollingRef = useRef<ReturnType<typeof setInterval> | null>(null)

  const stopPolling = useCallback(() => {
    if (pollingRef.current) {
      clearInterval(pollingRef.current)
      pollingRef.current = null
    }
  }, [])

  const loadProfile = useCallback(async () => {
    setProfileLoading(true)
    try {
      const next = await iywAccountGetProfile()
      setProfile(next)
    } catch {
      setProfile(null)
    } finally {
      setProfileLoading(false)
    }
  }, [])

  useEffect(() => {
    void loadProfile()
  }, [loadProfile])

  const loadQrcode = useCallback(async () => {
    stopPolling()
    setQrLoading(true)
    setQrError(null)
    setQrcode(null)
    setPollingStatus("loading")
    try {
      const next = await iywAccountGetWechatQrcode()
      setQrcode(next)
      setPollingStatus("pending")
    } catch (err) {
      setQrError(toErrorMessage(err))
      setPollingStatus("loading")
    } finally {
      setQrLoading(false)
    }
  }, [stopPolling])

  useEffect(() => {
    if (
      !open ||
      loginMode !== "wechat" ||
      profile?.logged_in ||
      profileLoading
    ) {
      return
    }
    void loadQrcode()
    return () => stopPolling()
  }, [
    loadQrcode,
    loginMode,
    open,
    profile?.logged_in,
    profileLoading,
    stopPolling,
  ])

  useEffect(() => {
    if (
      !open ||
      loginMode !== "wechat" ||
      !qrcode?.qr_token ||
      profile?.logged_in
    ) {
      return
    }
    pollingRef.current = setInterval(async () => {
      try {
        const result = await iywAccountPollWechatLogin(qrcode.qr_token)
        setPollingStatus(result.status)
        if (result.status === "success" && result.profile) {
          stopPolling()
          setProfile(result.profile)
          setQrcode(null)
          setOpen(false)
        }
      } catch (err) {
        setQrError(toErrorMessage(err))
      }
    }, POLL_INTERVAL_MS)
    return () => stopPolling()
  }, [loginMode, open, profile?.logged_in, qrcode?.qr_token, stopPolling])

  const handleLoginModeChange = useCallback(
    (mode: LoginMode) => {
      setLoginMode(mode)
      setQrError(null)
      setPasswordError(null)
      if (mode === "password") {
        stopPolling()
      }
    },
    [stopPolling]
  )

  useEffect(() => {
    if (!open) stopPolling()
  }, [open, stopPolling])

  const handleLogout = useCallback(async () => {
    setActionLoading(true)
    try {
      await iywAccountLogout()
      setProfile(null)
      setQrcode(null)
      setLoginMode("wechat")
      if (open) void loadQrcode()
    } finally {
      setActionLoading(false)
    }
  }, [loadQrcode, open])

  const handlePasswordLogin = useCallback(
    async (event: FormEvent<HTMLFormElement>) => {
      event.preventDefault()
      setPasswordLoading(true)
      setPasswordError(null)
      try {
        const next = await iywAccountLoginWithPassword({
          username,
          password,
        })
        stopPolling()
        setProfile(next)
        setQrcode(null)
        setPassword("")
        setUsername("")
        setOpen(false)
      } catch (err) {
        setPasswordError(toErrorMessage(err))
      } finally {
        setPasswordLoading(false)
      }
    },
    [password, stopPolling, username]
  )

  const title = displayName(profile, t("notSignedIn"))
  const subtitle = profile?.logged_in
    ? balancePoints(profile, t("balanceUnknown"))
    : t("clickToOpen")
  const dialogDescription = useMemo(
    () => (profile?.logged_in ? t("signedInDescription") : t("dialogHint")),
    [profile?.logged_in, t]
  )

  return (
    <>
      <div
        className={cn(
          "group flex min-h-14 w-full items-center gap-2.5 rounded-xl",
          "border border-sidebar-border/60 bg-sidebar-accent/55 px-2 py-2",
          "transition-colors hover:bg-sidebar-accent"
        )}
      >
        <button
          type="button"
          aria-haspopup="dialog"
          className={cn(
            "flex min-w-0 flex-1 items-center gap-2.5 rounded-lg text-left",
            "outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-inset"
          )}
          onClick={() => setOpen(true)}
        >
          <AccountAvatar profile={profile} className="h-8 w-8" />
          <div className="min-w-0 flex-1">
            <div className="truncate text-[0.8125rem] font-semibold leading-4 text-sidebar-foreground">
              {profileLoading ? t("loading") : title}
            </div>
            <div className="mt-1 flex min-w-0 items-center gap-1.5 text-[0.6875rem] leading-none text-muted-foreground">
              {profile?.logged_in ? (
                // eslint-disable-next-line @next/next/no-img-element
                <img
                  src="/iyw-points-icon.png"
                  alt=""
                  className="h-4 w-4 shrink-0 object-contain"
                  aria-hidden="true"
                />
              ) : (
                <span
                  aria-hidden="true"
                  className="h-1.5 w-1.5 shrink-0 rounded-full bg-muted-foreground/40"
                />
              )}
              <span
                className={cn("truncate", profile?.logged_in && "font-mono")}
              >
                {subtitle}
              </span>
            </div>
          </div>
        </button>

        <button
          type="button"
          aria-label={t("dialogTitle")}
          aria-haspopup="dialog"
          className={cn(
            "flex h-8 w-8 shrink-0 items-center justify-center rounded-lg",
            "border border-sidebar-border/70 bg-sidebar text-sidebar-foreground shadow-sm",
            "transition-colors hover:bg-background",
            "focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-inset"
          )}
          onClick={() => setOpen(true)}
        >
          <Settings className="h-3.5 w-3.5" aria-hidden="true" />
        </button>
      </div>

      <Dialog open={open} onOpenChange={setOpen}>
        <DialogContent className="max-w-lg overflow-hidden rounded-xl p-0">
          <DialogHeader className="px-5 pt-5">
            <DialogTitle>{t("dialogTitle")}</DialogTitle>
            <DialogDescription>{dialogDescription}</DialogDescription>
          </DialogHeader>

          <div className="min-h-[24rem] min-w-0 border-t border-border">
            {profile?.logged_in ? (
              <AccountProfilePanel
                profile={profile}
                loading={actionLoading}
                onLogout={handleLogout}
              />
            ) : (
              <AccountLoginPanel
                loginMode={loginMode}
                qrcode={qrcode}
                loading={qrLoading}
                error={qrError}
                pollingStatus={pollingStatus}
                onRefresh={loadQrcode}
                onLoginModeChange={handleLoginModeChange}
                username={username}
                password={password}
                passwordLoading={passwordLoading}
                passwordError={passwordError}
                onUsernameChange={(value) => {
                  setUsername(value)
                  setPasswordError(null)
                }}
                onPasswordChange={(value) => {
                  setPassword(value)
                  setPasswordError(null)
                }}
                onPasswordSubmit={handlePasswordLogin}
              />
            )}
          </div>
        </DialogContent>
      </Dialog>
    </>
  )
}
