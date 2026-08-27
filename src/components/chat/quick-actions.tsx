"use client"

import { useEffect, useMemo, useState } from "react"
import {
  BarChart3,
  BriefcaseBusiness,
  ChartNoAxesCombined,
  ClipboardList,
  FileSearch,
  Megaphone,
  Package,
  PenLine,
  Search,
  ShieldCheck,
  ShoppingCart,
  Sparkles,
  Users,
  type LucideIcon,
} from "lucide-react"
import { scenariosCatalog } from "@/lib/api"
import type { Scenario, ScenarioCatalog } from "@/lib/types"
import type { ComposerInjectContent } from "@/components/chat/message-input"
import { cn } from "@/lib/utils"
import { ScenarioPreferencesDialog } from "./scenario-preferences-dialog"
import { useScenarioPreferences } from "./scenario-preferences"

const ICONS: Record<string, LucideIcon> = {
  barChart3: BarChart3,
  briefcase: BriefcaseBusiness,
  chart: ChartNoAxesCombined,
  clipboard: ClipboardList,
  fileSearch: FileSearch,
  megaphone: Megaphone,
  package: Package,
  penLine: PenLine,
  search: Search,
  shield: ShieldCheck,
  shoppingCart: ShoppingCart,
  sparkles: Sparkles,
  users: Users,
}
const TONES: Record<string, string> = {
  amber: "border-amber-200 bg-amber-50/55 hover:border-amber-400",
  blue: "border-blue-200 bg-blue-50/55 hover:border-blue-400",
  green: "border-emerald-200 bg-emerald-50/55 hover:border-emerald-400",
  rose: "border-rose-200 bg-rose-50/55 hover:border-rose-400",
  slate: "border-slate-200 bg-slate-50/60 hover:border-slate-400",
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

function useOfficialScenarioCatalog() {
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
    void refresh()
    const onFocus = () => void refresh(true)
    window.addEventListener("focus", onFocus)
    return () => {
      active = false
      window.removeEventListener("focus", onFocus)
    }
  }, [])
  return { catalog, error }
}

interface QuickActionsProps {
  onSelect: (payload: ComposerInjectContent) => void
}

export function QuickActions({ onSelect }: QuickActionsProps) {
  const { catalog, error } = useOfficialScenarioCatalog()
  const { preferences, updatePreference, resetPreference } =
    useScenarioPreferences()
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
    <section className="flex flex-col gap-3" aria-label="官方场景">
      <div className="flex items-center gap-2">
        <div className="flex min-w-0 flex-1 gap-2 overflow-x-auto pb-1">
          {categories.map((category) => (
            <button
              key={category.key}
              type="button"
              onClick={() => setCategoryKey(category.key)}
              className={cn(
                "flex h-9 shrink-0 items-center gap-2 rounded-md border px-3 text-sm font-medium transition-colors",
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
        <ScenarioPreferencesDialog
          categories={categories}
          scenarios={catalog.scenarios}
          preferences={preferences}
          onUpdate={updatePreference}
          onReset={resetPreference}
        />
      </div>
      <div className="grid gap-3 sm:grid-cols-2 lg:grid-cols-3">
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
          <EmptyScenario text="当前分类中的场景已全部隐藏，可通过“管理场景”恢复。" />
        )}
      </div>
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
      onClick={() =>
        onSelect({
          text: prompt,
          skill: {
            id: scenario.skillPackageSlug,
            label: scenario.displayName,
            package: {
              id: scenario.skillPackageId,
              slug: scenario.skillPackageSlug,
              version: scenario.skillPackageVersion,
            },
          },
        })
      }
      className={cn(
        "group flex min-h-24 flex-col items-start gap-2 rounded-lg border px-4 py-3 text-left transition-[border-color,box-shadow,transform] hover:-translate-y-0.5 hover:shadow-sm focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/50",
        TONES[scenario.tone ?? "blue"] ?? TONES.blue
      )}
      aria-label={`${scenario.displayName}，使用 ${scenario.skillPackageSlug}`}
    >
      <span className="flex items-center gap-2">
        <span className="flex size-7 items-center justify-center rounded-md bg-background/80 text-foreground">
          <ScenarioIcon name={scenario.icon} className="size-4" />
        </span>
        <span className="text-sm font-semibold text-foreground">
          {scenario.displayName}
        </span>
      </span>
      <span className="line-clamp-2 text-xs leading-5 text-muted-foreground">
        {scenario.summary}
      </span>
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
