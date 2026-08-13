"use client"

import { AlertTriangle, Check, Loader2, PlugZap, RotateCcw } from "lucide-react"
import { useTranslations } from "next-intl"
import { Button } from "@/components/ui/button"
import {
  formatSkillBytes,
  type SkillMarketTranslator,
} from "@/lib/skill-market"

export function InstallMetric({
  label,
  value,
}: {
  label: string
  value: string
}) {
  return (
    <div>
      <p className="text-[10px] text-muted-foreground">{label}</p>
      <p className="mt-1 font-medium">{value}</p>
    </div>
  )
}

export function InstallBusy({ label }: { label: string }) {
  return (
    <div className="flex min-h-28 items-center justify-center gap-2 text-sm text-muted-foreground">
      <Loader2 className="size-4 animate-spin" aria-hidden="true" />
      {label}
    </div>
  )
}

export function ResultState({
  icon,
  title,
  detail,
  children,
}: {
  icon: React.ReactNode
  title: string
  detail?: string | null
  children: React.ReactNode
}) {
  return (
    <div className="space-y-3 py-3">
      <div className="flex items-center gap-2 text-sm font-medium">
        {icon}
        {title}
      </div>
      {detail ? (
        <p className="break-words border-l-2 border-destructive/40 pl-3 text-xs text-muted-foreground">
          {detail}
        </p>
      ) : null}
      <div className="flex items-center gap-2">{children}</div>
    </div>
  )
}

export function BusyPanel({
  message,
}: {
  message: "resolving" | "installingReal"
}) {
  const t = useTranslations("SkillMarketV2")
  return (
    <div className="p-4">
      <InstallBusy label={t(`install.${message}`)} />
    </div>
  )
}

export function InstallSuccess({
  includesConnectors,
  onClose,
  onOpenConnectors,
}: {
  includesConnectors: boolean
  onClose: () => void
  onOpenConnectors: () => void
}) {
  const t = useTranslations("SkillMarketV2")
  return (
    <div className="p-4">
      <ResultState
        icon={<Check className="size-5 text-emerald-500" />}
        title={t("install.done")}
      >
        {includesConnectors ? (
          <Button size="sm" variant="outline" onClick={onOpenConnectors}>
            <PlugZap className="size-3.5" />
            {t("install.openConnectors")}
          </Button>
        ) : null}
        <Button size="sm" onClick={onClose}>
          {t("install.doneClose")}
        </Button>
      </ResultState>
    </div>
  )
}

export function InstallFailure({
  errorMessage,
  errorAction,
  onRetry,
}: {
  errorMessage?: string | null
  errorAction: string | null
  onRetry: () => void
}) {
  const t = useTranslations("SkillMarketV2") as unknown as SkillMarketTranslator
  return (
    <div className="p-4">
      <ResultState
        icon={<AlertTriangle className="size-5 text-destructive" />}
        title={t("install.failed")}
        detail={errorMessage}
      >
        <Button size="sm" onClick={onRetry}>
          <RotateCcw className="size-3.5" />
          {t("install.actionRetry")}
        </Button>
        {errorAction && errorAction !== "retry" ? (
          <span className="text-[10px] text-muted-foreground">
            {t(`install.action${capitalize(errorAction)}`)}
          </span>
        ) : null}
      </ResultState>
    </div>
  )
}

export function InstallPlanMetrics({
  totalBytes,
  dependencyCount,
  targetCount,
}: {
  totalBytes: number
  dependencyCount: number
  targetCount: number
}) {
  const t = useTranslations("SkillMarketV2")
  return (
    <div className="grid grid-cols-3 gap-3 border-b pb-3 text-xs">
      <InstallMetric
        label={t("install.downloadSize")}
        value={formatSkillBytes(totalBytes)}
      />
      <InstallMetric
        label={t("detail.dependencies")}
        value={String(dependencyCount)}
      />
      <InstallMetric label={t("install.targets")} value={String(targetCount)} />
    </div>
  )
}

function capitalize(value: string): string {
  return value.charAt(0).toUpperCase() + value.slice(1)
}
