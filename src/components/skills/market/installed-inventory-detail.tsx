"use client"

import { RefreshCw } from "lucide-react"
import { useTranslations } from "next-intl"
import { Button } from "@/components/ui/button"
import { AgentMatrix } from "./installed-inventory-agents"
import { SkillLocations } from "./installed-inventory-locations"
import { cn } from "@/lib/utils"
import type { AgentType, LogicalSkillInventoryItem } from "@/lib/types"

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
