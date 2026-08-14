"use client"

import { Loader2, ShieldCheck } from "lucide-react"
import { useTranslations } from "next-intl"
import { Button } from "@/components/ui/button"
import { useAcpAgents } from "@/hooks/use-acp-agents"
import type { InventoryActions } from "./installed-inventory-detail"
import type {
  AgentType,
  LogicalSkillInventoryItem,
  SkillObservation,
  SkillObservedLocation,
} from "@/lib/types"

function LocationRow({
  skill,
  observation,
  location,
  agentType,
  busyKey,
  onTakeOver,
}: {
  skill: LogicalSkillInventoryItem
  observation: SkillObservation
  location: SkillObservedLocation
  agentType: AgentType | undefined
  busyKey: string | null
  onTakeOver: InventoryActions["onTakeOver"]
}) {
  const t = useTranslations("SkillMarketV2.inventory")
  const canTakeOver =
    skill.scope === "global" &&
    Boolean(agentType) &&
    (skill.conflict ||
      observation.ownership === "manual" ||
      observation.ownership === "agent_builtin")
  const busy =
    busyKey === `take-over:${skill.scope}:${skill.skillId}:${agentType}`
  const takeOver = () => {
    if (!agentType || !window.confirm(t("takeOverConfirm"))) return
    void onTakeOver(skill, observation.canonicalPath, agentType)
  }
  return (
    <div className="min-w-0 border-l-2 pl-2">
      <p className="break-all font-mono text-[11px]">{location.path}</p>
      <div className="mt-1 flex items-center gap-2">
        <p className="min-w-0 flex-1 text-[10px] text-muted-foreground">
          {location.enabled ? t("active") : t("inactive")}
        </p>
        {canTakeOver ? (
          <Button
            size="sm"
            variant="outline"
            className="h-7"
            disabled={busy}
            onClick={takeOver}
          >
            {busy ? (
              <Loader2 className="size-3.5 animate-spin" />
            ) : (
              <ShieldCheck className="size-3.5" />
            )}
            {t("takeOver")}
          </Button>
        ) : null}
      </div>
    </div>
  )
}

export function SkillLocations({
  skill,
  busyKey,
  onTakeOver,
}: {
  skill: LogicalSkillInventoryItem
  busyKey: string | null
  onTakeOver: InventoryActions["onTakeOver"]
}) {
  const t = useTranslations("SkillMarketV2.inventory")
  const { agents } = useAcpAgents()
  const fallbackAgent = agents.find(
    (agent) => agent.enabled && Boolean(agent.installed_version)
  )?.agent_type
  return (
    <section className="mt-4 border-t pt-3">
      <h3 className="text-xs font-medium">{t("locations")}</h3>
      <div className="mt-2 space-y-2">
        {skill.observations.flatMap((observation) =>
          observation.locations.map((location) => (
            <LocationRow
              key={location.path}
              skill={skill}
              observation={observation}
              location={location}
              agentType={location.agentTypes[0] ?? fallbackAgent}
              busyKey={busyKey}
              onTakeOver={onTakeOver}
            />
          ))
        )}
      </div>
    </section>
  )
}
