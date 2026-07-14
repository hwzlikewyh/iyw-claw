"use client"

import { useCallback, useMemo, useState } from "react"
import { Settings } from "lucide-react"
import { useTranslations } from "next-intl"

import { AccountLoginPanel } from "@/components/account/account-login-panel"
import {
  AccountAvatar,
  AccountProfilePanel,
  balancePoints,
  displayName,
} from "@/components/account/account-profile-panel"
import { Button } from "@/components/ui/button"
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"
import { useIywAccount } from "@/contexts/iyw-account-context"
import { openSettingsWindow } from "@/lib/api"
import { cn } from "@/lib/utils"

export { normalizeAvatarUrl } from "@/components/account/account-profile-panel"

export function SidebarAccountSettings() {
  const t = useTranslations("SidebarAccount")
  const startupT = useTranslations("StartupLogin")
  const [open, setOpen] = useState(false)
  const { status, profile, actionLoading, refreshProfile, logout } =
    useIywAccount()

  const handleOpenSettings = useCallback(() => {
    setOpen(false)
    openSettingsWindow("appearance").catch((error) => {
      console.error("[SidebarAccountSettings] failed to open settings:", error)
    })
  }, [])

  const title = displayName(profile, t("notSignedIn"))
  const subtitle = profile?.logged_in
    ? balancePoints(profile, t("balanceUnknown"))
    : t("clickToOpen")
  const description = useMemo(
    () => (profile?.logged_in ? t("signedInDescription") : t("dialogHint")),
    [profile?.logged_in, t]
  )

  return (
    <>
      <div
        className={cn(
          "group flex min-h-14 w-full items-center gap-2.5 rounded-lg",
          "border border-sidebar-border/60 bg-sidebar-accent/55 px-2 py-2",
          "transition-colors hover:bg-sidebar-accent"
        )}
      >
        <button
          type="button"
          aria-haspopup="dialog"
          className="flex min-w-0 flex-1 items-center gap-2.5 rounded-lg text-left outline-none focus-visible:ring-2 focus-visible:ring-ring"
          onClick={() => setOpen(true)}
        >
          <AccountAvatar profile={profile} className="h-8 w-8" />
          <div className="min-w-0 flex-1">
            <div className="truncate text-[0.8125rem] font-semibold leading-4 text-sidebar-foreground">
              {status === "checking" ? t("loading") : title}
            </div>
            <div className="mt-1 flex items-center gap-1.5 text-[0.6875rem] leading-none text-muted-foreground">
              {profile?.logged_in ? (
                // eslint-disable-next-line @next/next/no-img-element
                <img src="/iyw-points-icon.png" alt="" className="h-4 w-4" />
              ) : (
                <span className="h-1.5 w-1.5 rounded-full bg-muted-foreground/40" />
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
          aria-label={t("openSettings")}
          className="flex h-8 w-8 items-center justify-center rounded-lg border bg-sidebar shadow-sm hover:bg-background focus-visible:ring-2 focus-visible:ring-ring"
          onClick={handleOpenSettings}
        >
          <Settings className="h-3.5 w-3.5" />
        </button>
      </div>

      <Dialog open={open} onOpenChange={setOpen}>
        <DialogContent className="max-w-lg overflow-hidden rounded-lg p-0">
          <DialogHeader className="px-5 pt-5">
            <DialogTitle>{t("dialogTitle")}</DialogTitle>
            <DialogDescription>{description}</DialogDescription>
          </DialogHeader>
          <div className="min-h-[24rem] border-t">
            {status === "error" ? (
              <div className="grid min-h-80 place-items-center p-6 text-center">
                <div className="grid gap-3">
                  <p className="font-medium">{startupT("errorTitle")}</p>
                  <Button size="sm" onClick={() => void refreshProfile()}>
                    {startupT("retry")}
                  </Button>
                </div>
              </div>
            ) : profile?.logged_in ? (
              <AccountProfilePanel
                profile={profile}
                loading={actionLoading}
                onLogout={() => void logout()}
              />
            ) : (
              <AccountLoginPanel
                active={open}
                onAuthenticated={() => setOpen(false)}
              />
            )}
          </div>
        </DialogContent>
      </Dialog>
    </>
  )
}
