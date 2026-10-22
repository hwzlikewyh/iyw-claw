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
  displayName,
} from "@/components/account/account-profile-panel"
import { useWorkbenchRoute } from "@/contexts/workbench-route-context"
import type { IywAccountProfile } from "@/lib/types"
import { openSettingsWindow, type SettingsSection } from "@/lib/api"
import { copyTextFromMenu } from "@/lib/utils"
import { getCurrentAppVersion } from "@/lib/updater"
import { toErrorMessage } from "@/lib/app-error"
import { requestAppUpdateDialog } from "./sidebar-update-event"

import { SidebarAccountBalance } from "./sidebar-account-balance"

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
      <div className="flex min-w-0 items-center gap-3 px-4 pb-4 pt-5">
        <AccountAvatar
          profile={props.profile}
          className="size-10 ring-4 ring-primary/5"
        />
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
      <SidebarAccountBalance {...props} />
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
        className="mx-3 mb-3 h-9 w-[calc(100%-1.5rem)] justify-start gap-2 rounded-xl border border-transparent px-3 text-xs text-destructive/80 hover:border-destructive/10 hover:bg-destructive/5 hover:text-destructive"
        onClick={props.onLogout}
      >
        <LogOut className="size-3.5" />
        {account("logout")}
      </Button>
    </>
  )
}

function AccountShortcuts({ onClose }: { onClose: () => void }) {
  const t = useTranslations("SidebarDesign")
  const account = useTranslations("SidebarAccount")
  const { openConnectorMarket } = useWorkbenchRoute()
  const entries = [
    { section: "appearance", title: account("openSettings"), icon: Settings },
    { section: "usage", title: t("usage"), icon: ChartNoAxesCombined },
    { section: "connectors", title: t("connectors"), icon: Plug },
    { section: "user-memory", title: t("memory"), icon: Brain },
  ] as const
  const open = async (section: SettingsSection) => {
    onClose()
    try {
      if (section === "connectors") {
        openConnectorMarket()
        return
      }
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
    <div className="grid grid-cols-2 gap-2 px-4 py-3">
      {entries.map((entry) => (
        <button
          key={entry.section}
          type="button"
          onClick={() => void open(entry.section)}
          className="flex min-w-0 items-center gap-2 rounded-xl border border-transparent bg-muted/25 px-2.5 py-3 text-left text-xs transition-colors hover:border-primary/10 hover:bg-primary/5 focus-visible:outline focus-visible:outline-ring"
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
    <div className="flex items-center justify-between mx-4 mb-3 rounded-xl bg-muted/35 px-3 py-2.5 text-[11px]">
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
