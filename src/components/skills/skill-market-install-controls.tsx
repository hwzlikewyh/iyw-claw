"use client"

import { Loader2, RotateCcw, ShieldCheck, Trash2 } from "lucide-react"
import { useTranslations } from "next-intl"
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from "@/components/ui/alert-dialog"
import { Button } from "@/components/ui/button"
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select"
import { compareSemVer } from "@/components/skills/skill-market-semver"
import type { SkillMarketVersion } from "@/lib/skill-market"

export type InstallAction =
  | "install"
  | "update"
  | "reinstall"
  | "installVersion"

export function getInstallAction(
  installed: string | null,
  target: string
): InstallAction {
  if (!installed) return "install"
  if (installed === target) return "reinstall"
  return compareSemVer(target, installed) > 0 ? "update" : "installVersion"
}

type InstallControlsProps = {
  versions: SkillMarketVersion[]
  version: string
  action: InstallAction
  busy: boolean
  installDisabled: boolean
  installable: boolean
  versionsLoading: boolean
  versionsError: string | null
  onVersionChange: (version: string) => void
  onRetryVersions: () => void
  onConfirm: () => void
  onUninstall?: () => void
}

function VersionSelect(props: InstallControlsProps) {
  const selectVersion = (version: string) => {
    const selected = props.versions.find((item) => item.version === version)
    if (selected?.status === "ready") props.onVersionChange(version)
  }
  return (
    <Select
      value={props.version}
      disabled={props.versionsLoading}
      onValueChange={selectVersion}
    >
      <SelectTrigger className="w-full rounded-md">
        <SelectValue />
      </SelectTrigger>
      <SelectContent>
        {props.versions.map((item) => (
          <SelectItem
            key={item.id}
            value={item.version}
            disabled={item.status !== "ready"}
          >
            v{item.version}
          </SelectItem>
        ))}
      </SelectContent>
    </Select>
  )
}

function VersionsError({
  error,
  onRetry,
}: {
  error: string
  onRetry: () => void
}) {
  const t = useTranslations("SkillsSettings.market")
  if (!error) return null
  return (
    <div className="mt-2 flex items-start justify-between gap-2 rounded-md border border-destructive/30 bg-destructive/5 px-2.5 py-2">
      <p className="break-words text-[11px] text-destructive">
        {t("detail.versionsLoadFailed", { message: error })}
      </p>
      <Button
        size="icon-sm"
        variant="ghost"
        className="shrink-0"
        aria-label={t("detail.retryVersions")}
        title={t("detail.retryVersions")}
        onClick={onRetry}
      >
        <RotateCcw className="size-3.5" aria-hidden="true" />
      </Button>
    </div>
  )
}

function InstallHints(props: InstallControlsProps) {
  const t = useTranslations("SkillsSettings.market")
  return (
    <>
      {!props.installable ? (
        <p className="mt-2 text-[11px] text-destructive">
          {t("detail.versionUnavailable")}
        </p>
      ) : null}
      {props.installDisabled ? (
        <p className="mt-2 text-[11px] text-muted-foreground">
          {t("detail.installUnavailable")}
        </p>
      ) : null}
    </>
  )
}

export function InstallControls(props: InstallControlsProps) {
  const t = useTranslations("SkillsSettings.market")
  const disabled = props.busy || props.installDisabled || !props.installable
  return (
    <>
      <div className="mt-4 grid gap-2 sm:grid-cols-[1fr_auto]">
        <VersionSelect {...props} />
        <Button disabled={disabled} onClick={props.onConfirm}>
          {props.busy ? (
            <Loader2 className="size-3.5 animate-spin" />
          ) : (
            <ShieldCheck className="size-3.5" />
          )}
          {t(`actions.${props.action}`)}
        </Button>
      </div>
      <VersionsError
        error={props.versionsError ?? ""}
        onRetry={props.onRetryVersions}
      />
      <InstallHints {...props} />
      {props.onUninstall ? (
        <Button
          size="sm"
          variant="outline"
          className="mt-2 w-full text-destructive"
          onClick={props.onUninstall}
        >
          <Trash2 className="size-3.5" />
          {t("actions.uninstall")}
        </Button>
      ) : null}
    </>
  )
}

type InstallConfirmProps = {
  open: boolean
  action: InstallAction
  current: string | null
  target: string
  onOpenChange: (open: boolean) => void
  onInstall: () => void
}

export function InstallConfirm(props: InstallConfirmProps) {
  const t = useTranslations("SkillsSettings.market")
  return (
    <AlertDialog open={props.open} onOpenChange={props.onOpenChange}>
      <AlertDialogContent>
        <AlertDialogHeader>
          <AlertDialogTitle>
            {t(`confirm.${props.action}Title`)}
          </AlertDialogTitle>
          <AlertDialogDescription>
            {t("confirm.installDescription", {
              current: props.current ?? t("confirm.notInstalled"),
              target: props.target,
            })}
          </AlertDialogDescription>
        </AlertDialogHeader>
        <AlertDialogFooter>
          <AlertDialogCancel>{t("actions.cancel")}</AlertDialogCancel>
          <AlertDialogAction onClick={props.onInstall}>
            {t(`actions.${props.action}`)}
          </AlertDialogAction>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  )
}
