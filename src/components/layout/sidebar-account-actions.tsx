"use client"

import { useEffect, useState } from "react"
import { useTheme } from "next-themes"
import { useTranslations } from "next-intl"
import {
  Brain,
  ChartNoAxesCombined,
  Copy,
  ListTodo,
  LogOut,
  Monitor,
  Moon,
  Plug,
  RefreshCw,
  Settings,
  Sun,
  X,
} from "lucide-react"
import { toast } from "sonner"
import { Button } from "@/components/ui/button"
import {
  AccountAvatar,
  balancePoints,
  displayName,
} from "@/components/account/account-profile-panel"
import type { IywAccountProfile } from "@/lib/types"
import { openSettingsWindow, type SettingsSection } from "@/lib/api"
import { copyTextFromMenu } from "@/lib/utils"
import { getCurrentAppVersion } from "@/lib/updater"
import { toErrorMessage } from "@/lib/app-error"
import { requestAppUpdateDialog } from "./sidebar-update-event"

interface Props {
  profile: IywAccountProfile
  pending: boolean
  refreshing: boolean
  onRefresh: () => void
  onLogout: () => void
  onManage: () => void
  onProfile: () => void
  onClose: () => void
}

export function SidebarAccountActions(props: Props) {
  const t = useTranslations("SidebarDesign")
  const account = useTranslations("SidebarAccount")
  const common = useTranslations("Folder.common")
  const [version, setVersion] = useState("")
  useEffect(() => {
    let active = true
    void getCurrentAppVersion()
      .then((value) => {
        if (active) setVersion(value)
      })
      .catch(() => {})
    return () => {
      active = false
    }
  }, [])
  return (
    <>
      <div className="flex min-w-0 items-center gap-3 p-4">
        <AccountAvatar profile={props.profile} className="size-10" />
        <div className="min-w-0 flex-1">
          <div className="truncate text-sm font-semibold">
            {displayName(props.profile, account("signedIn"))}
          </div>
          <div className="flex min-w-0 items-center gap-1 text-xs text-muted-foreground">
            <span className="truncate">
              {props.profile.user_id || account("signedIn")}
            </span>
            {props.profile.user_id && (
              <Button
                variant="ghost"
                size="icon"
                className="size-6 shrink-0"
                title={t("copyId")}
                aria-label={t("copyId")}
                onClick={async () => {
                  if (await copyTextFromMenu(props.profile.user_id!))
                    toast.success(t("copied"))
                  else toast.error(t("copyFailed"))
                }}
              >
                <Copy className="size-3" />
              </Button>
            )}
          </div>
        </div>
        <Button
          variant="ghost"
          size="icon"
          className="size-6 shrink-0 self-start"
          title={common("close")}
          aria-label={common("close")}
          onClick={props.onClose}
        >
          <X className="size-3" />
        </Button>
      </div>
      <AccountBalance {...props} />
      <AccountShortcuts onClose={props.onClose} />
      <AccountTheme />
      <div className="flex flex-wrap items-center gap-1 border-t px-2 py-2">
        <span className="min-w-0 flex-1 px-1 text-[0.625rem] text-muted-foreground">
          {version ? "v" + version : ""}
        </span>
        <Button
          variant="ghost"
          size="sm"
          className="h-7 gap-1 px-2 text-[0.6875rem]"
          onClick={props.onManage}
        >
          <ListTodo className="size-3" />
          {t("sessions")}
        </Button>
        <AccountUpdate onClose={props.onClose} />
      </div>
      <Button
        variant="ghost"
        disabled={props.pending || props.refreshing}
        className="h-10 justify-start gap-2 rounded-none border-t px-4 text-xs text-destructive hover:bg-destructive/5 hover:text-destructive"
        onClick={props.onLogout}
      >
        <LogOut className="size-3.5" />
        {account("logout")}
      </Button>
    </>
  )
}

function AccountBalance(props: Props) {
  const t = useTranslations("SidebarAccount")
  const td = useTranslations("SidebarDesign")
  return (
    <div className="flex items-center justify-between gap-2 border-y bg-muted/30 px-4 py-3">
      <div className="min-w-0">
        <div className="text-xs text-muted-foreground">
          {t("balancePoints")}
        </div>
        <div className="break-words font-mono text-2xl font-medium">
          {balancePoints(props.profile, t("balanceUnknown"))}
        </div>
        {props.profile.balance_expiry_time && (
          <div className="mt-1 text-[0.625rem] text-muted-foreground">
            {t("balanceExpiry")}: {props.profile.balance_expiry_time}
          </div>
        )}
      </div>
      <div className="flex shrink-0 flex-col items-end gap-1">
        <Button
          variant="ghost"
          size="icon"
          className="size-7"
          title={t("refreshBalance")}
          aria-label={t("refreshBalance")}
          disabled={props.pending || props.refreshing}
          onClick={props.onRefresh}
        >
          <RefreshCw
            className={props.refreshing ? "size-3.5 animate-spin" : "size-3.5"}
          />
        </Button>
        <Button
          variant="ghost"
          size="sm"
          className="h-7 px-1 text-[0.6875rem]"
          onClick={props.onProfile}
        >
          {td("accountProfile")}
        </Button>
      </div>
    </div>
  )
}

function AccountShortcuts({ onClose }: { onClose: () => void }) {
  const t = useTranslations("SidebarDesign")
  const account = useTranslations("SidebarAccount")
  const entries = [
    { section: "appearance", title: account("openSettings"), icon: Settings },
    { section: "usage", title: t("usage"), icon: ChartNoAxesCombined },
    { section: "connectors", title: t("connectors"), icon: Plug },
    { section: "user-memory", title: t("memory"), icon: Brain },
  ] as const
  const open = async (section: SettingsSection) => {
    onClose()
    try {
      await openSettingsWindow(section)
    } catch (error) {
      console.error("[sidebar-account] open settings failed", {
        section,
        message: toErrorMessage(error),
      })
      toast.error(t("actionFailed", { message: toErrorMessage(error) }))
    }
  }
  return (
    <div className="grid grid-cols-2 gap-1 p-2">
      {entries.map((entry) => (
        <button
          key={entry.section}
          type="button"
          onClick={() => void open(entry.section)}
          className="flex min-w-0 items-center gap-2 rounded-md px-2 py-3 text-left text-xs hover:bg-accent focus-visible:outline focus-visible:outline-ring"
        >
          <span className="flex size-7 shrink-0 items-center justify-center rounded-md bg-primary/5 text-primary">
            <entry.icon className="size-3.5" />
          </span>
          <span className="truncate">{entry.title}</span>
        </button>
      ))}
    </div>
  )
}

function AccountTheme() {
  const t = useTranslations("SidebarDesign")
  const labels = useTranslations("AppearanceSettings")
  const { theme, setTheme } = useTheme()
  return (
    <div className="flex items-center justify-between border-t px-4 py-3 text-xs">
      <span>{t("appearance")}</span>
      <div
        className="flex gap-0.5 rounded-md bg-muted p-0.5"
        role="group"
        aria-label={t("appearance")}
      >
        {(
          [
            { id: "light", icon: Sun },
            { id: "dark", icon: Moon },
            { id: "system", icon: Monitor },
          ] as const
        ).map((entry) => (
          <Button
            key={entry.id}
            variant="ghost"
            size="icon"
            className={
              "h-6 w-7 rounded-sm " +
              (theme === entry.id ? "bg-background shadow-sm" : "")
            }
            title={labels(entry.id)}
            aria-label={labels(entry.id)}
            aria-pressed={theme === entry.id}
            onClick={() => setTheme(entry.id)}
          >
            <entry.icon className="size-3.5" />
          </Button>
        ))}
      </div>
    </div>
  )
}

function AccountUpdate({ onClose }: { onClose: () => void }) {
  const t = useTranslations("SystemSettings")
  return (
    <Button
      variant="ghost"
      size="sm"
      className="h-7 gap-1 px-2 text-[0.6875rem]"
      onClick={() => {
        onClose()
        requestAppUpdateDialog()
      }}
    >
      <RefreshCw className="size-3" />
      {t("checkUpdate")}
    </Button>
  )
}
