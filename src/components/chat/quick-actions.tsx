"use client"

import { useEffect, useMemo, useState } from "react"
import {
  BarChart3,
  BriefcaseBusiness,
  ChartNoAxesCombined,
  ClipboardList,
  Copyright,
  FileText,
  FileSearch,
  Flame,
  Image,
  Megaphone,
  Network,
  Package,
  Palette,
  PenLine,
  Search,
  ShieldCheck,
  ShoppingCart,
  Sparkles,
  Users,
  Video,
  type LucideIcon,
} from "lucide-react"
import { scenariosCatalog } from "@/lib/api"
import type { Scenario, ScenarioCatalog } from "@/lib/types"
import type { ComposerInjectContent } from "@/components/chat/message-input"
import { cn } from "@/lib/utils"
import { useScenarioPreferences } from "./scenario-preferences"
import { scenarioContent } from "./scenario-content"
import { ScenarioCases } from "./scenario-cases"
import { ScenarioCardIcon } from "./scenario-card-icon"
import cardStyles from "./scenario-card.module.css"
import layoutStyles from "./welcome-layout.module.css"

const ICONS: Record<string, LucideIcon> = {
  barChart3: BarChart3,
  briefcase: BriefcaseBusiness,
  chart: ChartNoAxesCombined,
  clipboard: ClipboardList,
  copyright: Copyright,
  fileText: FileText,
  fileSearch: FileSearch,
  flame: Flame,
  image: Image,
  megaphone: Megaphone,
  network: Network,
  package: Package,
  palette: Palette,
  penLine: PenLine,
  search: Search,
  shield: ShieldCheck,
  shoppingCart: ShoppingCart,
  sparkles: Sparkles,
  users: Users,
  video: Video,
}
let cachedCatalog: ScenarioCatalog | null = null
let catalogRequest: Promise<ScenarioCatalog> | null = null

function loadCatalog(force = false): Promise<ScenarioCatalog> {
  if (cachedCatalog && !force) return Promise.resolve(cachedCatalog)
  if (!catalogRequest) {
    catalogRequest = scenariosCatalog()
      .then((value) => {
        cachedCatalog = value
        return value
      })
      .finally(() => {
        catalogRequest = null
      })
  }
  return catalogRequest
}

function useOfficialScenarioCatalog(refreshKey?: string | number) {
  const [catalog, setCatalog] = useState<ScenarioCatalog | null>(cachedCatalog)
  const [error, setError] = useState<string | null>(null)
  useEffect(() => {
    let active = true
    const refresh = (force = false) =>
      loadCatalog(force)
        .then((value) => {
          if (!active) return
          setCatalog(value)
          setError(null)
        })
        .catch((cause: unknown) => {
          if (active)
            setError(cause instanceof Error ? cause.message : String(cause))
        })
    void refresh(true)
    const onFocus = () => void refresh(true)
    window.addEventListener("focus", onFocus)
    return () => {
      active = false
      window.removeEventListener("focus", onFocus)
    }
  }, [refreshKey])
  return { catalog, error }
}

interface QuickActionsProps {
  onSelect: (payload: ComposerInjectContent) => boolean | void
  refreshKey?: string | number
}

export function QuickActions({ onSelect, refreshKey }: QuickActionsProps) {
  const { catalog, error } = useOfficialScenarioCatalog(refreshKey)
  const { preferences } = useScenarioPreferences()
  const [categoryKey, setCategoryKey] = useState<string | null>(null)
  const categories = catalog?.categories ?? []
  const activeCategory =
    categoryKey ??
    categories.find((category) =>
      (catalog?.scenarios ?? []).some(
        (scenario) =>
          scenario.categoryKey === category.key &&
          !preferences[scenario.id]?.hidden
      )
    )?.key ??
    categories[0]?.key ??
    null
  const scenarios = useMemo(
    () =>
      (catalog?.scenarios ?? [])
        .filter((item) => item.categoryKey === activeCategory)
        .filter((item) => !preferences[item.id]?.hidden)
        .sort(
          (left, right) =>
            (preferences[left.id]?.sortOrder ?? left.sortOrder) -
            (preferences[right.id]?.sortOrder ?? right.sortOrder)
        ),
    [activeCategory, catalog?.scenarios, preferences]
  )
  if (error)
    return <EmptyScenario text="官方场景暂时不可用，请直接输入任务。" />
  if (!catalog)
    return <div className="h-28 animate-pulse rounded-lg bg-muted/30" />
  if (categories.length === 0)
    return <EmptyScenario text="暂无可用场景，请直接输入任务。" />
  return (
    <section className={layoutStyles.actions} aria-label="官方场景">
      <div className={layoutStyles.categories}>
        {categories.map((category) => (
          <button
            key={category.key}
            type="button"
            onClick={() => setCategoryKey(category.key)}
            className={cn(
              "flex shrink-0 items-center rounded-md border font-medium transition-colors",
              layoutStyles.category,
              activeCategory === category.key
                ? "border-primary bg-primary text-primary-foreground"
                : "border-border bg-muted/45 text-muted-foreground hover:bg-muted"
            )}
          >
            {category.icon ? (
              <ScenarioIcon name={category.icon} className="size-4" />
            ) : null}
            {category.displayName}
          </button>
        ))}
      </div>
      <div className={layoutStyles.grid}>
        {scenarios.length ? (
          scenarios.map((scenario) => (
            <ScenarioCard
              key={scenario.id}
              scenario={scenario}
              prompt={
                preferences[scenario.id]?.promptOverride ||
                scenario.promptTemplate
              }
              onSelect={onSelect}
            />
          ))
        ) : (
          <EmptyScenario text="当前分类暂无可用场景，请切换分类。" />
        )}
      </div>
      <ScenarioCases
        category={categories.find((item) => item.key === activeCategory)}
        preferences={preferences}
        refreshKey={refreshKey}
        onSelect={onSelect}
      />
    </section>
  )
}

function EmptyScenario({ text }: { text: string }) {
  return (
    <div className="rounded-lg border border-dashed px-4 py-5 text-center text-xs text-muted-foreground">
      {text}
    </div>
  )
}

function ScenarioCard({
  scenario,
  prompt,
  onSelect,
}: {
  scenario: Scenario
  prompt: string
  onSelect: (payload: ComposerInjectContent) => void
}) {
  return (
    <button
      type="button"
      onClick={() => onSelect(scenarioContent(scenario, prompt))}
      className={cardStyles.card}
      data-tone={scenario.tone ?? "blue"}
      aria-label={
        scenario.skillPackageSlug
          ? `${scenario.displayName}，使用 ${scenario.skillPackageSlug}`
          : scenario.displayName
      }
    >
      <span className={cardStyles.header}>
        <ScenarioCardIcon
          key={scenario.icon}
          source={scenario.icon}
          fallback={<ScenarioIcon name={scenario.icon} className="size-4" />}
        />
        <span className={cardStyles.title}>{scenario.displayName}</span>
      </span>
      <span className={cardStyles.summary}>{scenario.summary}</span>
      <span aria-hidden="true" className={cardStyles.accent} />
    </button>
  )
}

function ScenarioIcon({
  name,
  className,
}: {
  name: string | null | undefined
  className?: string
}) {
  const Icon = (name && ICONS[name]) || Sparkles
  return <Icon aria-hidden="true" className={className} />
}
