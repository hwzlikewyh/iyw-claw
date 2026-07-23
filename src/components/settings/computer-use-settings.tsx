"use client"

import { useCallback, useEffect, useMemo, useState } from "react"
import { Loader2, MonitorCog } from "lucide-react"
import { useLocale, useTranslations } from "next-intl"
import { toast } from "sonner"

import { Switch } from "@/components/ui/switch"
import {
  expertsList,
  managedSkillsGetFamilyState,
  managedSkillsSetSkillEnabled,
} from "@/lib/api"
import { toErrorMessage } from "@/lib/app-error"
import { pickLocalized } from "@/lib/expert-presentation"
import { invalidateAgentSkillsCache } from "@/hooks/use-agent-skills"
import type { ExpertListItem } from "@/lib/types"

const SKILL_ID = "open-computer-use"

export function ComputerUseSettings() {
  const t = useTranslations("ComputerUseSettings")
  const locale = useLocale()
  const [skill, setSkill] = useState<ExpertListItem | null>(null)
  const [enabled, setEnabled] = useState(false)
  const [loading, setLoading] = useState(true)
  const [saving, setSaving] = useState(false)
  const [loadError, setLoadError] = useState<string | null>(null)

  const refresh = useCallback(async () => {
    setLoading(true)
    setLoadError(null)
    try {
      const [experts, family] = await Promise.all([
        expertsList(),
        managedSkillsGetFamilyState("computer_use"),
      ])
      setSkill(experts.find((item) => item.metadata.id === SKILL_ID) ?? null)
      setEnabled(
        family.skills.find((item) => item.skillId === SKILL_ID)?.enabled ??
          false
      )
    } catch (error) {
      setLoadError(toErrorMessage(error))
    } finally {
      setLoading(false)
    }
  }, [])

  useEffect(() => {
    refresh().catch((error) => {
      console.error("[ComputerUseSettings] initial refresh failed:", error)
    })
  }, [refresh])

  const displayName = useMemo(
    () =>
      (skill && pickLocalized(skill.metadata.display_name, locale)) ||
      t("name"),
    [locale, skill, t]
  )
  const description = useMemo(
    () =>
      (skill && pickLocalized(skill.metadata.description, locale)) ||
      t("description"),
    [locale, skill, t]
  )

  const setComputerUseEnabled = useCallback(
    async (next: boolean) => {
      setSaving(true)
      try {
        const report = await managedSkillsSetSkillEnabled(
          "computer_use",
          SKILL_ID,
          next
        )
        setEnabled(report.enabled)
        report.touchedAgents.forEach((agent) =>
          invalidateAgentSkillsCache(agent)
        )
        toast.success(next ? t("enabled") : t("disabled"))
      } catch (error) {
        toast.error(t("saveFailed"), {
          description: toErrorMessage(error),
        })
        await refresh()
      } finally {
        setSaving(false)
      }
    },
    [refresh, t]
  )

  if (loading) {
    return (
      <div className="flex h-full items-center justify-center text-sm text-muted-foreground">
        <Loader2 className="mr-2 h-4 w-4 animate-spin" />
        {t("loading")}
      </div>
    )
  }

  return (
    <div className="h-full overflow-y-auto p-3 md:p-4">
      <div className="mx-auto w-full max-w-3xl">
        <div className="flex items-start justify-between gap-4 rounded-lg border bg-card p-4">
          <div className="flex min-w-0 gap-3">
            <MonitorCog className="mt-0.5 h-5 w-5 shrink-0 text-muted-foreground" />
            <div className="min-w-0">
              <h2 className="text-sm font-semibold">{displayName}</h2>
              <p className="mt-1 text-xs leading-5 text-muted-foreground">
                {description}
              </p>
              {loadError && (
                <p className="mt-2 text-xs text-destructive">{loadError}</p>
              )}
              {!skill?.installed_centrally && !loadError && (
                <p className="mt-2 text-xs text-amber-600 dark:text-amber-400">
                  {t("notReady")}
                </p>
              )}
            </div>
          </div>
          <div
            className="flex h-5 w-9 shrink-0 items-center justify-center"
            role={saving ? "status" : undefined}
            aria-label={saving ? t("loading") : undefined}
          >
            {saving ? (
              <Loader2
                aria-hidden="true"
                className="h-4 w-4 animate-spin text-muted-foreground"
              />
            ) : (
              <Switch
                aria-label={t("toggleLabel")}
                checked={enabled}
                disabled={!skill?.installed_centrally}
                onCheckedChange={(next) => {
                  setComputerUseEnabled(next).catch((error) => {
                    console.error("[ComputerUseSettings] toggle failed:", error)
                  })
                }}
              />
            )}
          </div>
        </div>
      </div>
    </div>
  )
}
