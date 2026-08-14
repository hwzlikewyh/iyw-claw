"use client"

import { AlertTriangle, Bot, Copy, Package, RefreshCw } from "lucide-react"
import { useTranslations } from "next-intl"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { AgentMatrix } from "./installed-inventory-agents"
import { SkillLocations } from "./installed-inventory-locations"
import { cn } from "@/lib/utils"
import type {
  AgentType,
  LogicalSkillInventoryItem,
  SkillInventoryStatus,
} from "@/lib/types"

export interface InventoryActions {
  onToggle: (
    skill: LogicalSkillInventoryItem,
    agentType: AgentType,
    enabled: boolean
  ) => Promise<void>
  onTakeOver: (
    skill: LogicalSkillInventoryItem,
    sourcePath: string,
    agentType: AgentType
  ) => Promise<void>
  onReconcile: () => Promise<void>
}

const STATUS_TONES: Record<SkillInventoryStatus, string> = {
  installed_active:
    "border-emerald-500/30 text-emerald-700 dark:text-emerald-300",
  installed_inactive: "text-muted-foreground",
  partial: "border-amber-500/30 text-amber-700 dark:text-amber-300",
  agent_builtin: "text-muted-foreground",
  duplicate: "border-amber-500/30 text-amber-700 dark:text-amber-300",
  conflict: "border-destructive/30 text-destructive",
  stale_market_record: "border-destructive/30 text-destructive",
  blocked: "border-destructive/30 text-destructive",
  out_of_sync: "border-amber-500/30 text-amber-700 dark:text-amber-300",
  unreadable: "border-destructive/30 text-destructive",
}

function StatusIcon({ status }: { status: SkillInventoryStatus }) {
  if (
    status === "conflict" ||
    status === "stale_market_record" ||
    status === "blocked" ||
    status === "unreadable"
  ) {
    return <AlertTriangle className="size-4" />
  }
  if (status === "duplicate") return <Copy className="size-4" />
  if (status === "agent_builtin") return <Bot className="size-4" />
  return <Package className="size-4" />
}

function InventoryBadges({ skill }: { skill: LogicalSkillInventoryItem }) {
  const t = useTranslations("SkillMarketV2.inventory")
  return (
    <span className="flex flex-wrap items-center gap-1.5">
      <span className="truncate text-sm font-medium">{skill.name}</span>
      <Badge variant="outline" className={STATUS_TONES[skill.status]}>
        {t(`status.${skill.status}`)}
      </Badge>
      {skill.localOnly ? (
        <Badge variant="secondary">{t("localOnly")}</Badge>
      ) : null}
      {skill.routingDescriptionOverLimit ? (
        <Badge
          variant="outline"
          className="border-amber-500/30 text-amber-700 dark:text-amber-300"
        >
          {t("descriptionOver")}
        </Badge>
      ) : null}
    </span>
  )
}

function InventoryMeta({
  skill,
  enabled,
}: {
  skill: LogicalSkillInventoryItem
  enabled: number
}) {
  const t = useTranslations("SkillMarketV2.inventory")
  return (
    <>
      <span className="mt-1 line-clamp-2 text-xs text-muted-foreground">
        {skill.description || t("noDescription")}
      </span>
      <span className="mt-1.5 block text-[11px] text-muted-foreground">
        {t(`scope.${skill.scope}`)} ·{" "}
        {t("agentCount", { enabled, total: skill.agentStates.length })}
      </span>
    </>
  )
}

export function InventoryRow({
  skill,
  selected,
  onSelect,
}: {
  skill: LogicalSkillInventoryItem
  selected: boolean
  onSelect: () => void
}) {
  const enabled = skill.agentStates.filter(
    (state) => state.actualEnabled
  ).length
  return (
    <button
      type="button"
      className={cn(
        "flex w-full items-start gap-3 border-b px-4 py-3 text-left transition-colors",
        selected ? "bg-muted/60" : "hover:bg-muted/30"
      )}
      onClick={onSelect}
    >
      <span className="mt-0.5 flex size-8 shrink-0 items-center justify-center border bg-background">
        <StatusIcon status={skill.status} />
      </span>
      <span className="min-w-0 flex-1">
        <InventoryBadges skill={skill} />
        <InventoryMeta skill={skill} enabled={enabled} />
      </span>
    </button>
  )
}

function InventorySummary({
  skill,
  busyKey,
  onReconcile,
}: {
  skill: LogicalSkillInventoryItem
  busyKey: string | null
  onReconcile: InventoryActions["onReconcile"]
}) {
  const t = useTranslations("SkillMarketV2.inventory")
  const canRepair = skill.status === "out_of_sync"
  return (
    <>
      <div className="flex items-start gap-2">
        <h2 className="min-w-0 flex-1 break-words text-base font-semibold">
          {skill.name}
        </h2>
        {canRepair ? (
          <Button
            size="icon-sm"
            variant="outline"
            disabled={Boolean(busyKey)}
            title={t("repair")}
            onClick={() => void onReconcile()}
          >
            <RefreshCw
              className={cn(
                "size-4",
                busyKey === "reconcile" && "animate-spin"
              )}
            />
          </Button>
        ) : null}
      </div>
      <p className="mt-1 break-all font-mono text-[11px] text-muted-foreground">
        {skill.skillId}
      </p>
      <p className="mt-3 text-sm leading-6 text-muted-foreground">
        {skill.description || t("noDescription")}
      </p>
      {skill.dependencies.length ? (
        <p className="mt-2 text-xs text-muted-foreground">
          {t("dependencies", { values: skill.dependencies.join(", ") })}
        </p>
      ) : null}
      {!skill.pluginAvailable ? (
        <p className="mt-2 text-xs text-destructive">
          {t("pluginUnavailable")}
        </p>
      ) : null}
    </>
  )
}

export function InventoryDetail({
  skill,
  busyKey,
  onToggle,
  onTakeOver,
  onReconcile,
}: {
  skill: LogicalSkillInventoryItem
  busyKey: string | null
} & InventoryActions) {
  return (
    <aside className="min-h-0 overflow-y-auto p-4 sm:p-5">
      <InventorySummary
        skill={skill}
        busyKey={busyKey}
        onReconcile={onReconcile}
      />
      <AgentMatrix skill={skill} busyKey={busyKey} onToggle={onToggle} />
      <SkillLocations skill={skill} busyKey={busyKey} onTakeOver={onTakeOver} />
    </aside>
  )
}
