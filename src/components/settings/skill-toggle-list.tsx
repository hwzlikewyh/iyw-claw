"use client"

import { useCallback, useEffect, useMemo, useState } from "react"
import { Loader2 } from "lucide-react"
import { useTranslations } from "next-intl"
import { toast } from "sonner"

import { Input } from "@/components/ui/input"
import { Switch } from "@/components/ui/switch"
import { toErrorMessage } from "@/lib/app-error"
import type { ManagedSkillState } from "@/lib/types"
import { SkillToggleCatalog } from "./skill-toggle-catalog"
import { SkillToggleDetailSheet } from "./skill-toggle-detail-sheet"
import {
  stripFrontmatter,
  type SkillToggleItem,
  type SkillToggleListProps,
} from "./skill-toggle-list-model"

export type { SkillToggleItem, SkillToggleListProps }

export function SkillToggleList({
  skills,
  skillStates,
  globalEnabled,
  setGlobalEnabled,
  setSkillEnabled,
  categoryOrder,
  translateCategory,
  loadContent,
  onApplied,
  searchPlaceholder,
  notReadyHint,
}: SkillToggleListProps) {
  const t = useTranslations("SkillMatrix")
  const [enabled, setEnabled] = useState(globalEnabled)
  const [states, setStates] = useState(
    () =>
      new Map<string, ManagedSkillState>(
        skillStates.map((state) => [state.skillId, state])
      )
  )
  const [pendingSkillIds, setPendingSkillIds] = useState(
    () => new Set<string>()
  )
  const [applying, setApplying] = useState(false)
  const [search, setSearch] = useState("")
  const [detailId, setDetailId] = useState<string | null>(null)
  const [detailContent, setDetailContent] = useState("")
  const [detailLoading, setDetailLoading] = useState(false)

  useEffect(() => {
    setEnabled(globalEnabled)
  }, [globalEnabled])

  useEffect(() => {
    setStates(new Map(skillStates.map((state) => [state.skillId, state])))
  }, [skillStates])

  useEffect(() => {
    if (!detailId || !loadContent) return
    let cancelled = false
    loadContent(detailId)
      .then((content) => {
        if (!cancelled) setDetailContent(stripFrontmatter(content))
      })
      .catch((error) => {
        if (!cancelled) {
          toast.error(t("toasts.loadFailed"), {
            description: toErrorMessage(error),
          })
        }
      })
      .finally(() => {
        if (!cancelled) setDetailLoading(false)
      })
    return () => {
      cancelled = true
    }
  }, [detailId, loadContent, t])

  const visibleSkills = useMemo(() => {
    const query = search.trim().toLowerCase()
    return skills
      .filter(
        (skill) =>
          !query ||
          skill.id.toLowerCase().includes(query) ||
          skill.displayName.toLowerCase().includes(query) ||
          skill.description.toLowerCase().includes(query)
      )
      .sort((left, right) => {
        const categoryDelta =
          (categoryOrder[left.category] ?? 99) -
          (categoryOrder[right.category] ?? 99)
        return (
          categoryDelta || left.displayName.localeCompare(right.displayName)
        )
      })
  }, [categoryOrder, search, skills])

  const groups = useMemo(() => {
    const grouped = new Map<string, SkillToggleItem[]>()
    for (const skill of visibleSkills) {
      grouped.set(skill.category, [
        ...(grouped.get(skill.category) ?? []),
        skill,
      ])
    }
    return Array.from(grouped.entries())
  }, [visibleSkills])

  const hasReadySkills = Array.from(states.values()).some(
    (state) => state.ready
  )

  const toggleAll = useCallback(
    async (nextEnabled: boolean) => {
      setApplying(true)
      try {
        const report = await setGlobalEnabled(nextEnabled)
        setEnabled(report.enabled)
        const okCount = report.results.filter((result) => result.ok).length
        const failCount = report.results.length - okCount
        if (failCount === 0) {
          toast.success(
            report.enabled
              ? t("toasts.enabled", { count: okCount })
              : t("toasts.disabled", { count: okCount })
          )
        } else {
          toast.warning(
            report.enabled
              ? t("toasts.enabledPartial", {
                  ok: okCount,
                  failed: failCount,
                })
              : t("toasts.disabledPartial", {
                  ok: okCount,
                  failed: failCount,
                }),
            {
              description:
                report.results.find((result) => !result.ok)?.error ?? undefined,
            }
          )
        }
        onApplied?.(report.touchedAgents)
      } catch (error) {
        toast.error(t("toasts.applyFailed"), {
          description: toErrorMessage(error),
        })
      } finally {
        setApplying(false)
      }
    },
    [onApplied, setGlobalEnabled, t]
  )

  const toggleSkill = useCallback(
    async (skillId: string, nextEnabled: boolean) => {
      const previousState = states.get(skillId)
      if (!previousState) return
      setPendingSkillIds((current) => new Set(current).add(skillId))
      setStates((current) => {
        const state = current.get(skillId)
        if (!state) return current
        return new Map(current).set(skillId, {
          ...state,
          enabled: nextEnabled,
        })
      })
      try {
        const report = await setSkillEnabled(skillId, nextEnabled)
        setStates((current) => {
          const state = current.get(skillId)
          if (!state) return current
          return new Map(current).set(skillId, {
            ...state,
            enabled: report.enabled,
          })
        })
        const okCount = report.results.filter((result) => result.ok).length
        const failCount = report.results.length - okCount
        if (failCount > 0) {
          toast.warning(
            report.enabled
              ? t("toasts.enabledPartial", {
                  ok: okCount,
                  failed: failCount,
                })
              : t("toasts.disabledPartial", {
                  ok: okCount,
                  failed: failCount,
                }),
            {
              description:
                report.results.find((result) => !result.ok)?.error ?? undefined,
            }
          )
        }
        onApplied?.(report.touchedAgents)
      } catch (error) {
        setStates((current) => {
          const state = current.get(skillId)
          if (!state) return current
          return new Map(current).set(skillId, {
            ...state,
            enabled: previousState.enabled,
          })
        })
        toast.error(t("toasts.applyFailed"), {
          description: toErrorMessage(error),
        })
      } finally {
        setPendingSkillIds((current) => {
          const next = new Set(current)
          next.delete(skillId)
          return next
        })
      }
    },
    [onApplied, setSkillEnabled, states, t]
  )

  const openDetail = useCallback((skillId: string) => {
    setDetailContent("")
    setDetailLoading(true)
    setDetailId(skillId)
  }, [])
  const detailSkill = skills.find((skill) => skill.id === detailId) ?? null

  return (
    <div className="flex h-full min-h-0 min-w-0 flex-col">
      <div className="mb-3 flex items-center gap-3">
        <Input
          value={search}
          onChange={(event) => setSearch(event.target.value)}
          placeholder={searchPlaceholder ?? t("searchPlaceholder")}
          aria-label={searchPlaceholder ?? t("searchPlaceholder")}
          className="max-w-xs"
        />
        <div className="ml-auto flex shrink-0 items-center gap-2">
          <label htmlFor="all-skills-toggle" className="text-sm font-medium">
            {t("columnMenu.enableAll")}
          </label>
          {applying ? (
            <Loader2 className="h-4 w-4 animate-spin text-muted-foreground" />
          ) : null}
          <Switch
            id="all-skills-toggle"
            checked={enabled}
            onCheckedChange={(next) => void toggleAll(next)}
            disabled={applying || (!hasReadySkills && !enabled)}
            aria-label={t("columnMenu.enableAll")}
            title={!hasReadySkills && !enabled ? notReadyHint : undefined}
          />
        </div>
      </div>
      <div className="min-h-0 flex-1 overflow-y-auto rounded-lg border bg-card">
        <SkillToggleCatalog
          groups={groups}
          skillStates={states}
          pendingSkillIds={pendingSkillIds}
          emptyText={search ? t("emptySearch") : t("empty")}
          previewEnabled={Boolean(loadContent)}
          notReadyHint={notReadyHint}
          translateCategory={translateCategory}
          onOpenDetail={openDetail}
          onToggleSkill={(skillId, nextEnabled) =>
            void toggleSkill(skillId, nextEnabled)
          }
        />
      </div>

      <SkillToggleDetailSheet
        skill={detailSkill}
        content={detailContent}
        loading={detailLoading}
        onClose={() => setDetailId(null)}
      />
    </div>
  )
}
