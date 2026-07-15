import { Download, Loader2, RefreshCw, Stethoscope, Trash2 } from "lucide-react"
import { useTranslations } from "next-intl"

import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import type { InternetToolInfo } from "@/lib/types"
import { cn } from "@/lib/utils"

interface InternetToolCardProps {
  name: string
  info: InternetToolInfo | null
  busy: boolean
  onInstall: () => void
  onUninstall: () => void
  onDoctor: () => void
  doctorLabel: string
}

export function InternetToolCard({
  name,
  info,
  busy,
  onInstall,
  onUninstall,
  onDoctor,
  doctorLabel,
}: InternetToolCardProps) {
  const t = useTranslations("InternetToolsSettings")
  const status = info?.status ?? "not_installed"
  const installed = info?.installed === true
  const statusLabel = {
    installed: t("installed"),
    update_available: t("updateAvailable"),
    not_runnable: t("notRunnable"),
    not_installed: t("notInstalled"),
  }[status]
  const installLabel = {
    installed: t("repair"),
    update_available: t("update"),
    not_runnable: t("repair"),
    not_installed: t("install"),
  }[status]

  return (
    <div
      className={cn(
        "min-w-0 border p-4",
        status === "installed"
          ? "border-green-500/30 bg-green-500/5"
          : status === "update_available" || status === "not_runnable"
            ? "border-amber-500/40 bg-amber-500/5"
            : "bg-card"
      )}
    >
      <div className="flex items-start justify-between gap-4">
        <div className="min-w-0">
          <div className="flex items-center gap-2">
            <h3 className="text-sm font-semibold">{name}</h3>
            <Badge variant="outline">{statusLabel}</Badge>
          </div>
          <p className="mt-1 text-xs text-muted-foreground">
            {t("expectedVersion", {
              version: info?.expectedVersion ?? "-",
            })}
          </p>
          {info?.version ? (
            <span className="text-[11px] text-muted-foreground">
              {info.version}
            </span>
          ) : null}
          {info?.path ? (
            <code className="mt-1 block break-all font-mono text-[10px] text-muted-foreground">
              {info.path}
            </code>
          ) : null}
          {info?.runtimeError ? (
            <p className="mt-2 whitespace-pre-wrap break-words text-xs text-amber-600 dark:text-amber-400">
              {info.runtimeError}
            </p>
          ) : null}
        </div>
        <div className="flex shrink-0 flex-wrap justify-end gap-2">
          {installed ? (
            <Button
              size="sm"
              variant="outline"
              disabled={busy}
              onClick={onDoctor}
            >
              <Stethoscope className="h-3.5 w-3.5" />
              {doctorLabel}
            </Button>
          ) : null}
          <Button size="sm" disabled={busy} onClick={onInstall}>
            {busy ? (
              <Loader2 className="h-3.5 w-3.5 animate-spin" />
            ) : status === "update_available" || installed ? (
              <RefreshCw className="h-3.5 w-3.5" />
            ) : (
              <Download className="h-3.5 w-3.5" />
            )}
            {installLabel}
          </Button>
          {installed ? (
            <Button
              size="sm"
              variant="outline"
              disabled={busy}
              className="text-destructive hover:text-destructive"
              onClick={onUninstall}
            >
              <Trash2 className="h-3.5 w-3.5" />
              {t("uninstall")}
            </Button>
          ) : null}
        </div>
      </div>
    </div>
  )
}
