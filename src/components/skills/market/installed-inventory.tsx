"use client"

import { useMemo, useState } from "react"
import { Loader2, RotateCcw, SearchX } from "lucide-react"
import { useTranslations } from "next-intl"
import { Button } from "@/components/ui/button"
import { ScrollArea } from "@/components/ui/scroll-area"
import { Skeleton } from "@/components/ui/skeleton"
import { InstalledInventoryCard } from "@/components/skills/market/installed-inventory-card"
import { InstalledInventoryDialog } from "@/components/skills/market/installed-inventory-dialog"
import type {
  LogicalSkillInventoryItem,
  SkillInventorySnapshot,
} from "@/lib/types"
import type { InventoryActions } from "@/components/skills/market/installed-inventory-detail"

interface InstalledInventoryProps extends InventoryActions {
  snapshot: SkillInventorySnapshot | null
  query: string
  enabledOnly: boolean
  loading: boolean
  error: string | null
  busyKey: string | null
  onRetry: () => void
}

function inventoryKey(skill: LogicalSkillInventoryItem) {
  return `${skill.scope}:${skill.skillId}`
}

function isProjectProvided(skill: LogicalSkillInventoryItem) {
  return skill.observations.every(
    (observation) =>
      observation.ownership === "agent_builtin" || observation.readOnly
  )
}

function InventoryListState({
  kind,
  error,
  onRetry,
}: {
  kind: "loading" | "empty" | "error"
  error?: string | null
  onRetry: () => void
}) {
  const t = useTranslations("SkillMarketV2.inventory")
  if (kind === "loading") {
    return (
      <div className="grid grid-cols-[repeat(auto-fill,minmax(min(100%,17rem),1fr))] gap-3 p-4 sm:p-5">
        {Array.from({ length: 6 }, (_, index) => (
          <Skeleton key={index} className="h-[11.75rem] rounded-lg" />
        ))}
      </div>
    )
  }
  return (
    <div className="flex h-full min-h-64 flex-col items-center justify-center gap-2 p-6 text-center">
      {kind === "empty" ? (
        <SearchX className="size-6 text-muted-foreground" aria-hidden="true" />
      ) : null}
      <p className="text-sm font-medium">
        {t(kind === "empty" ? "empty" : "loadFailed")}
      </p>
      {error ? <p className="text-xs text-muted-foreground">{error}</p> : null}
      {kind === "error" ? (
        <Button size="sm" variant="outline" onClick={onRetry}>
          <RotateCcw className="size-3.5" aria-hidden="true" />
          {t("retry")}
        </Button>
      ) : null}
    </div>
  )
}

function InventoryGridHeader({
  count,
  loading,
  budget,
}: {
  count: number
  loading: boolean
  budget: SkillInventorySnapshot["descriptionBudgets"][number] | undefined
}) {
  const t = useTranslations("SkillMarketV2.inventory")
  return (
    <>
      <div className="flex h-8 shrink-0 items-end px-4 pb-1 text-[10px] text-muted-foreground sm:px-5">
        {t("count", { count })}
        {loading ? <Loader2 className="ml-2 size-3 animate-spin" /> : null}
      </div>
      {budget ? (
        <div className="border-b bg-muted/20 px-4 py-2 text-[11px] text-muted-foreground sm:px-5">
          {t("budget", {
            used: budget.usedChars,
            limit: budget.softLimitChars,
            count: budget.skillCount,
          })}
        </div>
      ) : null}
    </>
  )
}

function InventoryCards({
  skills,
  selectedKey,
  onSelect,
}: {
  skills: LogicalSkillInventoryItem[]
  selectedKey: string | null
  onSelect: (skill: LogicalSkillInventoryItem) => void
}) {
  return (
    <ScrollArea className="min-h-0 flex-1">
      <div className="grid grid-cols-[repeat(auto-fill,minmax(min(100%,17rem),1fr))] gap-3 p-4 sm:p-5">
        {skills.map((skill) => (
          <InstalledInventoryCard
            key={inventoryKey(skill)}
            skill={skill}
            selected={inventoryKey(skill) === selectedKey}
            onSelect={() => onSelect(skill)}
          />
        ))}
      </div>
    </ScrollArea>
  )
}

function InventoryGrid({
  skills,
  selected,
  selectedKey,
  budget,
  loading,
  open,
  onSelect,
  onOpenChange,
  busyKey,
  onToggle,
  onTakeOver,
  onReconcile,
}: {
  skills: LogicalSkillInventoryItem[]
  selected: LogicalSkillInventoryItem | null
  selectedKey: string | null
  budget: SkillInventorySnapshot["descriptionBudgets"][number] | undefined
  loading: boolean
  open: boolean
  onSelect: (skill: LogicalSkillInventoryItem) => void
  onOpenChange: (open: boolean) => void
  busyKey: string | null
} & InventoryActions) {
  return (
    <div className="flex h-full min-h-0 flex-col">
      <InventoryGridHeader
        count={skills.length}
        loading={loading}
        budget={budget}
      />
      <InventoryCards
        skills={skills}
        selectedKey={selectedKey}
        onSelect={onSelect}
      />
      <InstalledInventoryDialog
        open={open && Boolean(selected)}
        onOpenChange={onOpenChange}
        skill={selected}
        busyKey={busyKey}
        onToggle={onToggle}
        onTakeOver={onTakeOver}
        onReconcile={onReconcile}
      />
    </div>
  )
}

export function InstalledInventoryView(props: InstalledInventoryProps) {
  const [selectedKey, setSelectedKey] = useState<string | null>(null)
  const [dialogOpen, setDialogOpen] = useState(false)
  const skills = useMemo(() => {
    const query = props.query.trim().toLocaleLowerCase()
    const all = (props.snapshot?.skills ?? []).filter(
      (skill) =>
        !isProjectProvided(skill) &&
        (!props.enabledOnly ||
          skill.agentStates.some((state) => state.actualEnabled))
    )
    if (!query) return all
    return all.filter((skill) =>
      `${skill.name} ${skill.skillId} ${skill.description ?? ""}`
        .toLocaleLowerCase()
        .includes(query)
    )
  }, [props.enabledOnly, props.query, props.snapshot?.skills])
  const selected =
    skills.find((skill) => inventoryKey(skill) === selectedKey) ?? null
  const budget = [...(props.snapshot?.descriptionBudgets ?? [])].sort(
    (left, right) => right.usedChars - left.usedChars
  )[0]

  if (props.loading && !props.snapshot) {
    return <InventoryListState kind="loading" onRetry={props.onRetry} />
  }
  if (props.error && !props.snapshot) {
    return (
      <InventoryListState
        kind="error"
        error={props.error}
        onRetry={props.onRetry}
      />
    )
  }
  if (!skills.length) {
    return <InventoryListState kind="empty" onRetry={props.onRetry} />
  }
  return (
    <InventoryGrid
      skills={skills}
      selected={selected}
      selectedKey={selectedKey}
      budget={budget}
      loading={props.loading}
      open={dialogOpen}
      onSelect={(skill) => {
        setSelectedKey(inventoryKey(skill))
        setDialogOpen(true)
      }}
      onOpenChange={setDialogOpen}
      busyKey={props.busyKey}
      onToggle={props.onToggle}
      onTakeOver={props.onTakeOver}
      onReconcile={props.onReconcile}
    />
  )
}
