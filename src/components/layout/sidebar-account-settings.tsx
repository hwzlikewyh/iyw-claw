"use client"

import { ChevronsUpDown, Loader2, Settings } from "lucide-react"
import { useTranslations } from "next-intl"
import {
  AccountAvatar,
  balancePoints,
  displayName,
} from "@/components/account/account-profile-panel"
import { Button } from "@/components/ui/button"
import {
  Popover,
  PopoverContent,
  PopoverTrigger,
} from "@/components/ui/popover"
import { cn } from "@/lib/utils"
import { useSidebarAccount } from "./use-sidebar-account"
import { SidebarAccountActions } from "./sidebar-account-actions"
import { SidebarAccountDialogs } from "./sidebar-account-dialogs"

export { normalizeAvatarUrl } from "@/components/account/account-profile-panel"

export function SidebarAccountSettings({
  compact = false,
}: {
  compact?: boolean
}) {
  const t = useTranslations("SidebarAccount")
  const account = useSidebarAccount()
  return (
    <>
      <div
        className={cn(
          "flex min-w-0 items-center gap-1 py-1",
          compact && "flex-col"
        )}
      >
        <Popover open={account.open} onOpenChange={account.setOpen}>
          <PopoverTrigger asChild>
            <button
              type="button"
              title={t("dialogTitle")}
              aria-label={t("dialogTitle")}
              className={cn(
                "flex min-w-0 flex-1 items-center gap-2 rounded-md p-1.5 text-left outline-none hover:bg-sidebar-accent focus-visible:ring-2 focus-visible:ring-ring",
                compact && "flex-none"
              )}
            >
              <AccountAvatar
                profile={account.profile}
                className="size-8 shrink-0"
              />
              {!compact && <AccountTriggerText account={account} />}
            </button>
          </PopoverTrigger>
          <PopoverContent
            side="top"
            align="start"
            sideOffset={10}
            onCloseAutoFocus={(event) => {
              if (account.dialog !== null) event.preventDefault()
            }}
            className="w-[21rem] max-w-[calc(100vw-1rem)] max-h-[var(--radix-popover-content-available-height)] gap-0 overflow-y-auto rounded-lg border p-0"
          >
            {account.error && account.profile?.logged_in && (
              <p
                role="alert"
                className="break-words border-b p-3 text-xs text-destructive"
              >
                {account.error}
              </p>
            )}
            {account.profile?.logged_in ? (
              <SidebarAccountActions
                profile={account.profile}
                pending={account.actionLoading}
                refreshing={account.refreshing}
                onRefresh={() => void account.refresh()}
                onLogout={() => account.showDialog("logout")}
                onManage={() => account.showDialog("manager")}
                onProfile={() => account.showDialog("profile")}
                onClose={() => account.setOpen(false)}
              />
            ) : (
              <AccountUnavailable account={account} />
            )}
          </PopoverContent>
        </Popover>
        <Button
          variant="ghost"
          size="icon"
          className="size-8 shrink-0 rounded-md text-muted-foreground"
          title={t("openSettings")}
          aria-label={t("openSettings")}
          onClick={() => void account.settings()}
        >
          <Settings className="size-3.5" />
        </Button>
      </div>
      <SidebarAccountDialogs account={account} />
    </>
  )
}

function AccountTriggerText({
  account,
}: {
  account: ReturnType<typeof useSidebarAccount>
}) {
  const t = useTranslations("SidebarAccount")
  return (
    <>
      <span className="min-w-0 flex-1">
        <span className="block truncate text-xs font-semibold">
          {account.status === "checking"
            ? t("loading")
            : displayName(account.profile, t("notSignedIn"))}
        </span>
        <span className="mt-0.5 block truncate text-[0.6875rem] text-muted-foreground">
          {account.profile?.logged_in
            ? t("balancePoints") +
              " · " +
              balancePoints(account.profile, t("balanceUnknown"))
            : t("clickToOpen")}
        </span>
      </span>
      <ChevronsUpDown className="size-3 shrink-0 text-muted-foreground" />
    </>
  )
}

function AccountUnavailable({
  account,
}: {
  account: ReturnType<typeof useSidebarAccount>
}) {
  const t = useTranslations("SidebarAccount")
  const td = useTranslations("SidebarDesign")
  return (
    <div className="grid gap-3 p-4">
      <div className="flex items-center gap-2 text-sm">
        {account.status === "checking" && (
          <Loader2 className="size-4 animate-spin" />
        )}
        {t(account.status === "checking" ? "loading" : "notSignedIn")}
      </div>
      {account.error && (
        <p role="alert" className="break-words text-xs text-destructive">
          {account.error}
        </p>
      )}
      <Button
        size="sm"
        disabled={account.status === "checking"}
        onClick={() =>
          account.status === "error"
            ? void account.refresh()
            : account.showDialog("login")
        }
      >
        {account.status === "error" ? td("retry") : t("signIn")}
      </Button>
      <Button
        variant="ghost"
        size="sm"
        onClick={() => account.showDialog("manager")}
      >
        {td("sessions")}
      </Button>
    </div>
  )
}
