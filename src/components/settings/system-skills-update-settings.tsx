"use client"

import { useCallback, useEffect, useState } from "react"
import {
  CheckCircle2,
  KeyRound,
  Loader2,
  PackageCheck,
  RefreshCw,
  RotateCcw,
} from "lucide-react"
import { useLocale, useTranslations } from "next-intl"
import { toast } from "sonner"
import { Button } from "@/components/ui/button"
import { Switch } from "@/components/ui/switch"
import { openSettingsWindow } from "@/lib/api"
import { toErrorMessage } from "@/lib/app-error"
import {
  applySystemSkillsUpdate,
  checkSystemSkillsUpdate,
  getSystemSkillsUpdateState,
  rollbackSystemSkillsUpdate,
  setSystemSkillsAutoUpdate,
  subscribeSystemSkillsUpdate,
  type SystemSkillsUpdateState,
  type SystemSkillsUpdateStatus,
} from "@/lib/system-skills-update"

const BUSY_STATUSES = new Set<SystemSkillsUpdateStatus>([
  "checking",
  "downloading",
  "validating",
  "applying",
])

function versionLabel(value: string | null): string {
  if (!value) return "-"
  return value.startsWith("v") ? value : `v${value}`
}

function formatLastChecked(locale: string, value: string | null | undefined) {
  if (!value) return null
  const date = new Date(value)
  if (Number.isNaN(date.getTime())) return null
  return new Intl.DateTimeFormat(locale, {
    dateStyle: "medium",
    timeStyle: "short",
  }).format(date)
}

export function SystemSkillsUpdateSettings() {
  const t = useTranslations("SystemSettings")
  const locale = useLocale()
  const [state, setState] = useState<SystemSkillsUpdateState | null>(null)
  const [loading, setLoading] = useState(true)

  const acceptState = useCallback((next: SystemSkillsUpdateState) => {
    setState((current) =>
      !current || next.seq >= current.seq ? next : current
    )
  }, [])

  useEffect(() => {
    let active = true
    let unsubscribe: (() => void) | null = null
    void subscribeSystemSkillsUpdate((next) => active && acceptState(next))
      .then((dispose) => {
        if (active) unsubscribe = dispose
        else dispose()
      })
      .catch((error) =>
        console.warn("[system-skills] subscribe failed", { error })
      )
    void getSystemSkillsUpdateState()
      .then((next) => active && acceptState(next))
      .catch((error) =>
        console.warn("[system-skills] state load failed", { error })
      )
      .finally(() => active && setLoading(false))
    return () => {
      active = false
      unsubscribe?.()
    }
  }, [acceptState])

  const run = useCallback(
    async (action: () => Promise<SystemSkillsUpdateState>) => {
      try {
        acceptState(await action())
      } catch (error) {
        const message = toErrorMessage(error)
        toast.error(t("systemSkillsActionFailed", { message }))
      }
    },
    [acceptState, t]
  )

  const lastChecked = formatLastChecked(locale, state?.lastCheckedAt)
  const busy = state ? BUSY_STATUSES.has(state.status) : false
  const statusKey = state?.status ?? "idle"

  return (
    <section className="space-y-4 border-t pt-4">
      <div className="flex items-center gap-2">
        {busy ? (
          <Loader2 className="h-4 w-4 animate-spin text-muted-foreground" />
        ) : state?.status === "up_to_date" ? (
          <CheckCircle2 className="h-4 w-4 text-green-500" />
        ) : (
          <PackageCheck className="h-4 w-4 text-muted-foreground" />
        )}
        <h2 className="text-sm font-semibold">{t("systemSkillsTitle")}</h2>
      </div>

      <p className="text-xs leading-5 text-muted-foreground">
        {t("systemSkillsDescription")}
      </p>

      <div className="space-y-3 border-y py-3 text-xs">
        <div className="flex flex-wrap items-center justify-between gap-2">
          <span className="text-muted-foreground">
            {t("systemSkillsCurrentVersion")}
          </span>
          <span className="font-medium tabular-nums">
            {versionLabel(state?.currentVersion ?? null)}
          </span>
        </div>
        <div className="flex flex-wrap items-center justify-between gap-2">
          <span className="text-muted-foreground">
            {t("systemSkillsLatestVersion")}
          </span>
          <span className="font-medium tabular-nums">
            {versionLabel(state?.latestVersion ?? null)}
          </span>
        </div>
        <div className="flex items-center justify-between gap-3">
          <label htmlFor="system-skills-auto-update" className="min-w-0">
            <span className="block font-medium">
              {t("systemSkillsAutoUpdate")}
            </span>
            <span className="block text-[11px] leading-5 text-muted-foreground">
              {t("systemSkillsAutoUpdateHint")}
            </span>
          </label>
          <Switch
            id="system-skills-auto-update"
            checked={state?.autoUpdate ?? true}
            disabled={loading || busy}
            onCheckedChange={(enabled) =>
              void run(() => setSystemSkillsAutoUpdate(enabled))
            }
          />
        </div>
      </div>

      <div className="flex flex-wrap items-center gap-2">
        <Button
          size="sm"
          variant="outline"
          disabled={loading || busy}
          onClick={() => void run(checkSystemSkillsUpdate)}
        >
          <RefreshCw className="h-3.5 w-3.5" />
          {t("systemSkillsCheck")}
        </Button>
        {(state?.status === "update_available" ||
          (state?.status === "error" && state.latestVersion)) && (
          <Button
            size="sm"
            disabled={busy}
            onClick={() => void run(applySystemSkillsUpdate)}
          >
            <PackageCheck className="h-3.5 w-3.5" />
            {t("systemSkillsInstall")}
          </Button>
        )}
        {state?.previousVersion && (
          <Button
            size="sm"
            variant="outline"
            disabled={busy || state.dirty}
            onClick={() => void run(rollbackSystemSkillsUpdate)}
          >
            <RotateCcw className="h-3.5 w-3.5" />
            {t("systemSkillsRollback")}
          </Button>
        )}
        <Button
          size="sm"
          variant="ghost"
          onClick={() => void openSettingsWindow("version-control")}
        >
          <KeyRound className="h-3.5 w-3.5" />
          {t("systemSkillsCredentials")}
        </Button>
      </div>

      <p className="text-[11px] leading-5 text-muted-foreground">
        {t(`systemSkillsStatus.${statusKey}`)}
        {lastChecked
          ? ` · ${t("systemSkillsLastChecked", { time: lastChecked })}`
          : ""}
      </p>

      {(state?.error || state?.dirty) && (
        <div className="rounded-md border border-red-500/30 bg-red-500/5 px-3 py-2 text-xs text-red-400">
          {state.dirty
            ? t("systemSkillsDirty")
            : t("systemSkillsActionFailed", { message: state.error ?? "" })}
        </div>
      )}
    </section>
  )
}
