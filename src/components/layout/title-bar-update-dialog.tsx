"use client"

import {
  ArrowUpCircle,
  CheckCircle2,
  Download,
  Loader2,
  RefreshCw,
  RotateCcw,
  ShieldAlert,
} from "lucide-react"
import { useTranslations } from "next-intl"

import { AppIcon } from "@/components/app-icon"
import { TitleBarUpdateReleaseDetails } from "@/components/layout/title-bar-update-release-details"
import type { UpdateContextValue } from "@/components/providers/update-provider"
import {
  ACTIVE_UPDATE_STATUSES,
  getLifecycleErrorKind,
  getUpdateErrorKey,
  getVisibleUpdate,
  type UpdateDialogState,
} from "@/components/layout/title-bar-update-model"
import { Button } from "@/components/ui/button"
import {
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"
import { openUrl } from "@/lib/platform"
import type { AppUpdateState } from "@/lib/updater"
import { cn } from "@/lib/utils"

const LATEST_RELEASE_URL =
  "https://github.com/hwzlikewyh/iyw-claw/releases/latest"

interface UpdateDialogProps {
  update: UpdateContextValue
  dialog: UpdateDialogState
}

interface StatusLineProps {
  icon: typeof ArrowUpCircle
  label: string
  spinning?: boolean
  tone?: "default" | "success" | "error"
}

function StatusLine({
  icon: Icon,
  label,
  spinning,
  tone = "default",
}: StatusLineProps) {
  return (
    <div
      aria-live="polite"
      className={cn(
        "flex items-start gap-2 text-sm text-muted-foreground",
        tone === "success" && "text-emerald-600 dark:text-emerald-400",
        tone === "error" && "text-destructive"
      )}
    >
      <Icon
        className={cn("mt-0.5 size-4 shrink-0", spinning && "animate-spin")}
      />
      <span className="min-w-0 break-words">{label}</span>
    </div>
  )
}

function UpdateStatus({ update, dialog }: UpdateDialogProps) {
  const t = useTranslations("SystemSettings")
  const { state } = update
  const lifecycleError = getLifecycleErrorKind(state, dialog.details)
  const errorKind = dialog.checkErrorKind ?? lifecycleError
  if (
    dialog.checking ||
    state.status === "checking" ||
    (!update.hydrated && !dialog.details.loaded)
  ) {
    return <StatusLine icon={Loader2} spinning label={t("checking")} />
  }
  if (errorKind) {
    const action = dialog.checkErrorKind ? "check" : "install"
    return (
      <StatusLine
        icon={ShieldAlert}
        tone="error"
        label={t(getUpdateErrorKey(errorKind, action))}
      />
    )
  }
  if (state.status === "restarting" || update.isRestarting) {
    return <StatusLine icon={Loader2} spinning label={t("restarting")} />
  }
  if (state.status === "ready_to_restart") {
    return <StatusLine icon={RotateCcw} label={t("updateReadyHint")} />
  }
  if (state.status === "downloading") {
    return <StatusLine icon={Loader2} spinning label={t("downloading")} />
  }
  if (state.status === "verifying" || state.status === "installing") {
    return <StatusLine icon={Loader2} spinning label={t("updating")} />
  }
  const visibleUpdate = getVisibleUpdate(state, dialog.details)
  if (visibleUpdate) {
    return (
      <StatusLine
        icon={ArrowUpCircle}
        label={t("foundUpdate", { version: visibleUpdate.version })}
      />
    )
  }
  return (
    <StatusLine icon={CheckCircle2} tone="success" label={t("alreadyLatest")} />
  )
}

function UpdateProgress({ state }: { state: AppUpdateState }) {
  const t = useTranslations("SystemSettings")
  if (!["downloading", "verifying", "installing"].includes(state.status)) {
    return null
  }
  const percent =
    state.status === "downloading" && state.total && state.total > 0
      ? Math.min(100, Math.round(((state.downloaded ?? 0) / state.total) * 100))
      : null
  const label =
    state.status === "downloading" ? t("downloading") : t("updating")

  return (
    <div className="space-y-2">
      <div className="flex items-center justify-between text-xs text-muted-foreground">
        <span>{label}</span>
        {percent !== null ? <span>{percent}%</span> : null}
      </div>
      <div className="h-1.5 overflow-hidden rounded-full bg-muted">
        <div
          className={cn(
            "h-full rounded-full bg-primary transition-[width] duration-300",
            percent === null && "w-1/3 animate-pulse"
          )}
          style={percent === null ? undefined : { width: `${percent}%` }}
        />
      </div>
    </div>
  )
}

function openLatestRelease() {
  void openUrl(LATEST_RELEASE_URL).catch((error) => {
    console.error("[TitleBarUpdate] failed to open release page:", error)
  })
}

function BusyUpdateAction({ checking }: { checking: boolean }) {
  const t = useTranslations("SystemSettings")
  return (
    <Button disabled>
      <Loader2 className="animate-spin" />
      {t(checking ? "checking" : "updating")}
    </Button>
  )
}

function isUpdateActionBusy({ update, dialog }: UpdateDialogProps): boolean {
  return (
    update.isBusy ||
    dialog.checking ||
    ACTIVE_UPDATE_STATUSES.has(update.state.status)
  )
}

function UpdateAction({ update, dialog }: UpdateDialogProps) {
  const t = useTranslations("SystemSettings")
  const { state } = update
  if (state.status === "ready_to_restart") {
    return (
      <Button onClick={() => void update.restart()} disabled={update.isBusy}>
        <RotateCcw />
        {t("restartToUpdate")}
      </Button>
    )
  }
  if (isUpdateActionBusy({ update, dialog })) {
    return (
      <BusyUpdateAction
        checking={state.status === "checking" || dialog.checking}
      />
    )
  }
  const visibleUpdate = dialog.checkErrorKind
    ? null
    : getVisibleUpdate(state, dialog.details)
  if (visibleUpdate) {
    if (!dialog.details.loaded) {
      return (
        <Button disabled>
          <Loader2 className="animate-spin" />
          {t("checking")}
        </Button>
      )
    }
    return dialog.details.canInstall ? (
      <Button onClick={() => void update.startUpdate()}>
        <Download />
        {t("upgradeTo", { version: visibleUpdate.version })}
      </Button>
    ) : (
      <Button onClick={openLatestRelease}>
        <ArrowUpCircle />
        {t("viewRelease", { version: visibleUpdate.version })}
      </Button>
    )
  }
  return (
    <Button onClick={() => void dialog.runCheck()} disabled={update.isBusy}>
      <RefreshCw />
      {t("checkUpdate")}
    </Button>
  )
}

export function TitleBarUpdateDialog({ update, dialog }: UpdateDialogProps) {
  const t = useTranslations("SystemSettings")
  const visibleUpdate = dialog.checkErrorKind
    ? null
    : getVisibleUpdate(update.state, dialog.details)
  return (
    <DialogContent className="max-w-lg rounded-lg">
      <DialogHeader>
        <DialogTitle className="flex items-center gap-2">
          <AppIcon className="size-5" />
          {t("versionTitle")}
        </DialogTitle>
        <DialogDescription>{t("updateDescription")}</DialogDescription>
      </DialogHeader>
      <div className="space-y-4 border-y py-4">
        <div className="flex items-center justify-between gap-4 text-sm">
          <span className="text-muted-foreground">{t("currentVersion")}</span>
          <span className="font-mono">
            {dialog.details.currentVersion
              ? `v${dialog.details.currentVersion}`
              : "-"}
          </span>
        </div>
        <UpdateStatus update={update} dialog={dialog} />
        <UpdateProgress state={update.state} />
        <TitleBarUpdateReleaseDetails
          update={visibleUpdate}
          details={dialog.details}
        />
      </div>
      <div className="flex justify-end">
        <UpdateAction update={update} dialog={dialog} />
      </div>
    </DialogContent>
  )
}
