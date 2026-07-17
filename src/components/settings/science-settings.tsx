"use client"

import { useCallback, useEffect, useMemo, useState } from "react"
import { FolderOpen, Loader2, RefreshCw } from "lucide-react"
import { useLocale, useTranslations } from "next-intl"
import { toast } from "sonner"

import {
  SkillToggleList,
  type SkillToggleItem,
} from "@/components/settings/skill-toggle-list"
import {
  mergeAllManagedSkillsEnabled,
  mergeManagedSkillEnabled,
} from "@/components/settings/skill-toggle-list-model"
import { Button } from "@/components/ui/button"
import { invalidateAgentSkillsCache } from "@/hooks/use-agent-skills"
import {
  managedSkillsGetFamilyState,
  managedSkillsGetGlobalState,
  managedSkillsSetGlobalEnabled,
  managedSkillsSetSkillEnabled,
  openFolder,
  scienceList,
  scienceOpenCentralDir,
  scienceReadContent,
} from "@/lib/api"
import { toErrorMessage } from "@/lib/app-error"
import { pickLocalized } from "@/lib/expert-presentation"
import { revealItemInDir } from "@/lib/platform"
import { getActiveRemoteConnectionId, isDesktop } from "@/lib/transport"
import type { ManagedSkillFamilyState, ScienceListItem } from "@/lib/types"

const CATEGORY_SORT: Record<string, number> = {
  ideation: 1,
  design: 2,
  analysis: 3,
  visualization: 4,
  evaluation: 5,
  literature: 6,
}

export function ScienceSettings() {
  const t = useTranslations("ScienceSettings")
  const locale = useLocale()
  const [skills, setSkills] = useState<ScienceListItem[]>([])
  const [globalEnabled, setGlobalEnabled] = useState(false)
  const [familyState, setFamilyState] =
    useState<ManagedSkillFamilyState | null>(null)
  const [loading, setLoading] = useState(true)
  const [loadError, setLoadError] = useState<string | null>(null)

  const refresh = useCallback(async () => {
    setLoading(true)
    setLoadError(null)
    try {
      const [catalog, globalState, state] = await Promise.all([
        scienceList(),
        managedSkillsGetGlobalState(),
        managedSkillsGetFamilyState("science"),
      ])
      setSkills(catalog)
      setGlobalEnabled(globalState.scienceEnabled)
      setFamilyState(state)
    } catch (error) {
      setLoadError(toErrorMessage(error))
      setSkills([])
      setFamilyState(null)
    } finally {
      setLoading(false)
    }
  }, [])

  useEffect(() => {
    refresh().catch((error) => {
      console.error("[ScienceSettings] initial refresh failed:", error)
    })
  }, [refresh])

  const translateCategory = useCallback(
    (category: string) => {
      if (category in CATEGORY_SORT) {
        return t(`categories.${category}` as Parameters<typeof t>[0])
      }
      return category
    },
    [t]
  )

  const toggleSkills = useMemo<SkillToggleItem[]>(
    () =>
      skills.map((skill) => ({
        id: skill.metadata.id,
        category: skill.metadata.category,
        displayName:
          pickLocalized(skill.metadata.display_name, locale) ||
          skill.metadata.id,
        description: pickLocalized(skill.metadata.description, locale),
        ready: skill.installed_centrally,
        badge: skill.user_modified
          ? { label: t("badges.userModified"), tone: "amber" }
          : skill.metadata.needs_key
            ? { label: t("badges.needsKey"), tone: "amber" }
            : skill.metadata.needs_env
              ? { label: t("badges.needsSetup"), tone: "muted" }
              : undefined,
      })),
    [locale, skills, t]
  )

  const openCentralDirectory = useCallback(async () => {
    try {
      const path = await scienceOpenCentralDir()
      if (isDesktop() && getActiveRemoteConnectionId() === null) {
        await revealItemInDir(path)
      } else {
        await openFolder(path)
      }
    } catch (error) {
      toast.error(t("toasts.openFolderFailed"), {
        description: toErrorMessage(error),
      })
    }
  }, [t])

  if (loading) {
    return (
      <div className="flex h-full items-center justify-center text-sm text-muted-foreground">
        <Loader2 className="mr-2 h-4 w-4 animate-spin" />
        {t("loading")}
      </div>
    )
  }

  return (
    <div className="flex h-full flex-col p-3 md:p-4">
      <div className="flex items-center justify-between gap-3 pb-4">
        <div>
          <h2 className="text-base font-semibold">{t("title")}</h2>
          <p className="mt-1 text-xs text-muted-foreground">
            {t("description")}
          </p>
        </div>
        <div className="flex items-center gap-2">
          <Button size="sm" variant="outline" onClick={openCentralDirectory}>
            <FolderOpen className="h-3.5 w-3.5" />
            {t("actions.openCentralDir")}
          </Button>
          <Button size="sm" variant="outline" onClick={refresh}>
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
        <div className="flex h-full items-center justify-center rounded-lg border bg-card text-sm text-muted-foreground">
          {t("emptySkills")}
        </div>
      ) : (
        <div className="min-h-0 min-w-0 flex-1">
          <SkillToggleList
            skills={toggleSkills}
            skillStates={familyState?.skills ?? []}
            globalEnabled={familyState?.allEnabled ?? globalEnabled}
            setGlobalEnabled={async (enabled) => {
              const report = await managedSkillsSetGlobalEnabled(
                "science",
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
                "science",
                skillId,
                enabled
              )
              setFamilyState((current) =>
                mergeManagedSkillEnabled(current, skillId, report.enabled)
              )
              return report
            }}
            categoryOrder={CATEGORY_SORT}
            translateCategory={translateCategory}
            loadContent={scienceReadContent}
            onApplied={(agents) =>
              agents.forEach((agent) => invalidateAgentSkillsCache(agent))
            }
            searchPlaceholder={t("searchPlaceholder")}
          />
        </div>
      )}
    </div>
  )
}
