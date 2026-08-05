"use client"

import { useCallback, useEffect, useState } from "react"
import { CheckCircle2, Loader2, PackageCheck, RefreshCw } from "lucide-react"
import { useLocale, useTranslations } from "next-intl"
import { toast } from "sonner"
import { Button } from "@/components/ui/button"
import { toErrorMessage } from "@/lib/app-error"
import {
  applySystemSkillsUpdate,
  checkSystemSkillsUpdate,
  getSystemSkillsUpdateState,
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
        <p className="text-[11px] leading-5 text-muted-foreground">
          {t("systemSkillsAutoUpdateNotice")}
        </p>
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
      </div>

      <p className="text-[11px] leading-5 text-muted-foreground">
        {t(`systemSkillsStatus.${statusKey}`)}
        {lastChecked
          ? ` · ${t("systemSkillsLastChecked", { time: lastChecked })}`
          : ""}
      </p>

      {state?.dirty && (
        <div className="rounded-md border border-amber-500/30 bg-amber-500/5 px-3 py-2 text-xs text-amber-500">
          {t("systemSkillsDirty")}
        </div>
      )}

      {state?.error && (
        <div className="rounded-md border border-red-500/30 bg-red-500/5 px-3 py-2 text-xs text-red-400">
          {t("systemSkillsActionFailed", { message: state.error })}
        </div>
      )}
    </section>
  )
}
