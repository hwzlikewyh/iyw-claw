"use client"

import { useCallback, useEffect, useMemo, useState } from "react"
import { Loader2 } from "lucide-react"
import { useTranslations } from "next-intl"
import { toast } from "sonner"

import { Input } from "@/components/ui/input"
import { Switch } from "@/components/ui/switch"
import { toErrorMessage } from "@/lib/app-error"
import type { ExpertInstallStatus, LinkOp, LinkOpResult } from "@/lib/types"
import { SkillToggleCatalog } from "./skill-toggle-catalog"
import { SkillToggleDetailSheet } from "./skill-toggle-detail-sheet"
import {
  buildStatusMap,
  isBlocked,
  isEnabled,
  statusKey,
  stripFrontmatter,
  type SkillToggleItem,
  type SkillToggleListProps,
} from "./skill-toggle-list-model"

export type { SkillToggleItem, SkillToggleListProps }

export function SkillToggleList({
  skills,
  agents,
  categoryOrder,
  translateCategory,
  loadAllStatuses,
  applyLinks,
  loadContent,
  onApplied,
  statusReloadToken = 0,
  searchPlaceholder,
  notReadyHint,
}: SkillToggleListProps) {
  const t = useTranslations("SkillMatrix")
  const [statuses, setStatuses] = useState(
    () => new Map<string, ExpertInstallStatus>()
  )
  const [loading, setLoading] = useState(true)
  const [applying, setApplying] = useState(false)
  const [search, setSearch] = useState("")
  const [detailId, setDetailId] = useState<string | null>(null)
  const [detailContent, setDetailContent] = useState("")
  const [detailLoading, setDetailLoading] = useState(false)

  const refreshStatuses = useCallback(async () => {
    setStatuses(buildStatusMap(await loadAllStatuses()))
  }, [loadAllStatuses])

  useEffect(() => {
    let cancelled = false
    loadAllStatuses()
      .then((next) => {
        if (!cancelled) setStatuses(buildStatusMap(next))
      })
      .catch((error) => {
        if (!cancelled) {
          toast.error(t("toasts.loadFailed"), {
            description: toErrorMessage(error),
          })
        }
      })
      .finally(() => {
        if (!cancelled) setLoading(false)
      })
    return () => {
      cancelled = true
    }
  }, [loadAllStatuses, statusReloadToken, t])

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

  const targets = useMemo(
    () =>
      skills
        .filter((skill) => skill.ready)
        .flatMap((skill) =>
          agents.flatMap((agent) => {
            const key = statusKey(skill.id, agent.agent_type)
            const status = statuses.get(key)
            if (
              !statuses.has(key) ||
              (!isEnabled(status) && isBlocked(status))
            ) {
              return []
            }
            return [{ skillId: skill.id, agentType: agent.agent_type }]
          })
        ),
    [agents, skills, statuses]
  )
  const enabled =
    targets.length > 0 &&
    targets.every(({ skillId, agentType }) =>
      isEnabled(statuses.get(statusKey(skillId, agentType)))
    )

  const toggleAll = useCallback(
    async (enable: boolean) => {
      const ops = targets.flatMap(({ skillId, agentType }): LinkOp[] => {
        const current = isEnabled(statuses.get(statusKey(skillId, agentType)))
        return current === enable
          ? []
          : [{ expertId: skillId, agentType, enable }]
      })
      if (ops.length === 0) return

      setApplying(true)
      let results: LinkOpResult[] = []
      let applyError: unknown = null
      try {
        results = await applyLinks(ops)
      } catch (error) {
        applyError = error
      }
      try {
        await refreshStatuses()
      } catch (error) {
        console.warn("[SkillToggleList] status reconcile failed:", error)
      }
      if (applyError) {
        toast.error(t("toasts.applyFailed"), {
          description: toErrorMessage(applyError),
        })
      } else {
        const okCount = results.filter((result) => result.ok).length
        const failCount = results.length - okCount
        if (failCount === 0) {
          toast.success(
            enable
              ? t("toasts.enabled", { count: okCount })
              : t("toasts.disabled", { count: okCount })
          )
        } else {
          toast.warning(
            enable
              ? t("toasts.enabledPartial", { ok: okCount, failed: failCount })
              : t("toasts.disabledPartial", { ok: okCount, failed: failCount }),
            {
              description:
                results.find((result) => !result.ok)?.error ?? undefined,
            }
          )
        }
      }
      setApplying(false)
      onApplied?.([...new Set(ops.map((op) => op.agentType))])
    },
    [applyLinks, onApplied, refreshStatuses, statuses, t, targets]
  )

  const openDetail = useCallback((skillId: string) => {
    setDetailContent("")
    setDetailLoading(true)
    setDetailId(skillId)
  }, [])
  const detailSkill = skills.find((skill) => skill.id === detailId) ?? null

  if (loading) {
    return (
      <div className="flex h-full items-center justify-center text-sm text-muted-foreground">
        <Loader2 className="mr-2 h-4 w-4 animate-spin" />
        {t("loading")}
      </div>
    )
  }

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
            disabled={applying || targets.length === 0}
            aria-label={t("columnMenu.enableAll")}
            title={targets.length === 0 ? notReadyHint : undefined}
          />
        </div>
      </div>
      <div className="min-h-0 flex-1 overflow-y-auto rounded-lg border bg-card">
        <SkillToggleCatalog
          groups={groups}
          emptyText={search ? t("emptySearch") : t("empty")}
          previewEnabled={Boolean(loadContent)}
          notReadyHint={notReadyHint}
          translateCategory={translateCategory}
          onOpenDetail={openDetail}
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
