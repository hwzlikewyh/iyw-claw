"use client"

import { useState } from "react"
import { useTranslations } from "next-intl"
import { Download, HardDrive, Loader2, Trash2 } from "lucide-react"
import { toast } from "sonner"
import { SettingCard, SettingRow } from "@/components/computer/settings-layout"
import {
  SettingsError,
  SettingsSection,
} from "@/components/computer/settings-layout"
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
import { Progress } from "@/components/ui/progress"
import { useComputerDriver } from "@/lib/computer/use-computer-driver"
import { progressOf } from "./progress-of"

export function DriverSection() {
  const t = useTranslations("ComputerUse.settings.driver")
  const { info, error, install, uninstall } = useComputerDriver()
  const [confirmUninstall, setConfirmUninstall] = useState(false)
  const [pending, setPending] = useState(false)
  const task = info?.task
  const busy = task !== undefined || pending
  const current = !!info && info.installed.includes(info.version)
  const older = info ? info.installed.filter((v) => v !== info.version) : []
  const progress = progressOf(info)
  const problem = error ?? info?.error ?? null
  let line: string
  if (!info) line = t("loading")
  else if (!info.supported) line = t("unsupported")
  else if (task?.kind === "installing")
    line =
      progress.done !== null
        ? progress.total !== null
          ? t("downloadingOf", {
              done: progress.done,
              total: progress.total.toFixed(1),
            })
          : t("downloadingSoFar", { done: progress.done })
        : t("installing")
  else if (task?.kind === "uninstalling") line = t("uninstalling")
  else if (current) line = t("installed", { version: info.version })
  else if (older.length > 0)
    line = t("outdated", { installed: older[0], version: info.version })
  else line = t("notInstalled")
  const onInstall = async () => {
    if (busy) return
    setPending(true)
    try {
      if (await install()) toast.success(t("installDone"))
    } finally {
      setPending(false)
    }
  }
  const onUninstall = async () => {
    if (busy) return
    setPending(true)
    try {
      if (await uninstall()) toast.success(t("uninstallDone"))
    } finally {
      setPending(false)
      setConfirmUninstall(false)
    }
  }
  return (
    <SettingsSection
      icon={HardDrive}
      title={t("title")}
      description={t("description", { version: info?.version ?? "" })}
    >
      <SettingCard>
        <SettingRow
          title="cua-driver"
          description={line}
          control={
            busy ? (
              <Loader2 className="size-4 animate-spin text-muted-foreground" />
            ) : info?.supported ? (
              <div className="flex items-center gap-1">
                {!current && (
                  <Button size="sm" onClick={() => void onInstall()}>
                    <Download className="size-3.5" />
                    {older.length > 0
                      ? t("upgrade", { version: info.version })
                      : t("install")}
                  </Button>
                )}
                {info.installed.length > 0 && (
                  <Button
                    size="sm"
                    variant="outline"
                    onClick={() => setConfirmUninstall(true)}
                  >
                    <Trash2 className="size-3.5" />
                    {t("uninstall")}
                  </Button>
                )}
              </div>
            ) : null
          }
        >
          {progress.value !== null && (
            <Progress value={progress.value} className="h-1.5" />
          )}
          {current && info?.path && (
            <p
              className="truncate font-mono text-2xs text-muted-foreground"
              title={info.path}
            >
              {info.path}
            </p>
          )}
        </SettingRow>
      </SettingCard>

      {problem && <SettingsError>{problem}</SettingsError>}

      <AlertDialog
        open={confirmUninstall}
        onOpenChange={(open) => {
          if (!busy) setConfirmUninstall(open)
        }}
      >
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>{t("confirmTitle")}</AlertDialogTitle>
            <AlertDialogDescription>
              {t("confirmDescription")}
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel disabled={busy}>{t("cancel")}</AlertDialogCancel>

            <AlertDialogAction
              disabled={busy}
              onClick={(event) => {
                event.preventDefault()
                void onUninstall()
              }}
            >
              {t("uninstall")}
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </SettingsSection>
  )
}
