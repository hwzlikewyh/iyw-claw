"use client"

import { useCallback, useEffect, useMemo, useState } from "react"
import { Loader2, RefreshCw } from "lucide-react"
import { useLocale, useTranslations } from "next-intl"

import {
  SkillToggleList,
  type SkillToggleItem,
} from "@/components/settings/skill-toggle-list"
import {
  mergeAllManagedSkillsEnabled,
  mergeManagedSkillEnabled,
} from "@/components/settings/skill-toggle-list-model"
import { Button } from "@/components/ui/button"
import {
  expertsList,
  expertsReadContent,
  managedSkillsGetFamilyState,
  managedSkillsGetGlobalState,
  managedSkillsSetGlobalEnabled,
  managedSkillsSetSkillEnabled,
} from "@/lib/api"
import { invalidateAgentSkillsCache } from "@/hooks/use-agent-skills"
import type { ExpertListItem, ManagedSkillFamilyState } from "@/lib/types"
import { toErrorMessage } from "@/lib/app-error"
import {
  CODEX_NATIVE_CATEGORY,
  pickLocalized,
} from "@/lib/expert-presentation"

/**
 * The codex-native managed family: bundled replacements for the skills
 * Codex CLI ships under `~/.codex/skills/.system/`. Published to Codex only;
 * while any replacement is enabled, reconcile keeps the `.system` copies
 * cleared so sessions never see duplicates.
 */
export function CodexNativeSettings() {
  const t = useTranslations("CodexNativeSettings")
  const locale = useLocale()

  const [skills, setSkills] = useState<ExpertListItem[]>([])
  const [globalEnabled, setGlobalEnabled] = useState(false)
  const [familyState, setFamilyState] =
    useState<ManagedSkillFamilyState | null>(null)
  const [loading, setLoading] = useState(true)
  const [loadError, setLoadError] = useState<string | null>(null)

  const refresh = useCallback(async () => {
    setLoading(true)
    setLoadError(null)
    try {
      const [expertList, globalState, nextFamilyState] = await Promise.all([
        expertsList(),
        managedSkillsGetGlobalState(),
        managedSkillsGetFamilyState("codex_native"),
      ])
      setSkills(
        expertList.filter(
          (item) => item.metadata.category === CODEX_NATIVE_CATEGORY
        )
      )
      setGlobalEnabled(globalState.codexNativeEnabled)
      setFamilyState(nextFamilyState)
    } catch (err) {
      setLoadError(toErrorMessage(err))
      setSkills([])
      setFamilyState(null)
    } finally {
      setLoading(false)
    }
  }, [])

  useEffect(() => {
    refresh().catch((err) => {
      console.error("[CodexNativeSettings] initial refresh failed:", err)
    })
  }, [refresh])

  const translatedCategory = useCallback(
    (category: string): string =>
      category === CODEX_NATIVE_CATEGORY ? t("category") : category,
    [t]
  )

  const toggleSkills = useMemo<SkillToggleItem[]>(
    () =>
      skills.map((e) => ({
        id: e.metadata.id,
        category: e.metadata.category,
        displayName:
          pickLocalized(e.metadata.display_name, locale) || e.metadata.id,
        description: pickLocalized(e.metadata.description, locale),
        ready: e.installed_centrally,
        badge: e.user_modified
          ? { label: t("badges.userModified"), tone: "amber" }
          : undefined,
      })),
    [skills, locale, t]
  )

  if (loading) {
    return (
      <div className="h-full flex items-center justify-center text-sm text-muted-foreground">
        <Loader2 className="h-4 w-4 mr-2 animate-spin" />
        {t("loading")}
      </div>
    )
  }

  return (
    <div className="h-full flex flex-col p-3 md:p-4">
      <div className="flex flex-col items-start justify-between gap-3 pb-4 sm:flex-row sm:items-center">
        <div className="min-w-0">
          <h2 className="text-base font-semibold">{t("title")}</h2>
          <p className="text-xs text-muted-foreground mt-1">
            {t("description")}
          </p>
        </div>
        <div className="flex flex-wrap items-center gap-2">
          <Button
            size="sm"
            variant="outline"
            onClick={() => {
              refresh().catch((err) => {
                console.error("[CodexNativeSettings] refresh failed:", err)
              })
            }}
          >
            <RefreshCw className="h-3.5 w-3.5" />
            {t("actions.refresh")}
          </Button>
        </div>
      </div>

      {loadError && (
        <div className="mb-3 rounded-md border border-red-500/30 bg-red-500/5 px-3 py-2 text-xs text-red-400">
          {loadError}
        </div>
      )}

      {skills.length === 0 ? (
        <div className="flex h-full items-center justify-center rounded-lg border bg-card px-4 text-center text-sm text-muted-foreground">
          {t("empty")}
        </div>
      ) : (
        <div className="flex-1 min-h-0 min-w-0">
          <SkillToggleList
            skills={toggleSkills}
            skillStates={familyState?.skills ?? []}
            globalEnabled={familyState?.allEnabled ?? globalEnabled}
            setGlobalEnabled={async (enabled) => {
              const report = await managedSkillsSetGlobalEnabled(
                "codex_native",
                enabled
              )
              setGlobalEnabled(report.enabled)
              setFamilyState((current) =>
                mergeAllManagedSkillsEnabled(current, report.enabled)
              )
              return report
            }}
            setSkillEnabled={async (skillId, enabled) => {
              const report = await managedSkillsSetSkillEnabled(
                "codex_native",
                skillId,
                enabled
              )
              setFamilyState((current) =>
                mergeManagedSkillEnabled(current, skillId, report.enabled)
              )
              return report
            }}
            categoryOrder={{ [CODEX_NATIVE_CATEGORY]: 1 }}
            translateCategory={translatedCategory}
            loadContent={expertsReadContent}
            onApplied={(touched) => {
              touched.forEach((a) => invalidateAgentSkillsCache(a))
            }}
            searchPlaceholder={t("searchPlaceholder")}
          />
        </div>
      )}
    </div>
  )
}
