"use client"

import { Loader2 } from "lucide-react"
import { useTranslations } from "next-intl"
import { Switch } from "@/components/ui/switch"
import { useAcpAgents } from "@/hooks/use-acp-agents"
import { AGENT_LABELS } from "@/lib/types"
import type { InventoryActions } from "./installed-inventory-detail"
import type { AgentType, LogicalSkillInventoryItem } from "@/lib/types"

function AgentToggle({
  agentType,
  skill,
  blocked,
  busyKey,
  onToggle,
}: {
  agentType: AgentType
  skill: LogicalSkillInventoryItem
  blocked: boolean
  busyKey: string | null
  onToggle: InventoryActions["onToggle"]
}) {
  const t = useTranslations("SkillMarketV2.inventory")
  const state = skill.agentStates.find((item) => item.agentType === agentType)
  const busy = busyKey === `${skill.scope}:${skill.skillId}:${agentType}`
  const required = Boolean(state?.requiredBy.length)
  return (
    <label className="flex min-h-11 items-center gap-3 py-2 text-xs">
      <span className="min-w-0 flex-1 truncate">
        {AGENT_LABELS[agentType]}
        {required ? (
          <span className="ml-2 text-[10px] text-muted-foreground">
            {t("requiredDependency")}
          </span>
        ) : null}
      </span>
      {busy ? <Loader2 className="size-4 animate-spin" /> : null}
      <Switch
        checked={state?.effectiveEnabled ?? false}
        disabled={blocked || Boolean(busyKey) || required}
        onCheckedChange={(value) => void onToggle(skill, agentType, value)}
      />
    </label>
  )
}

export function AgentMatrix({
  skill,
  busyKey,
  onToggle,
}: {
  skill: LogicalSkillInventoryItem
  busyKey: string | null
  onToggle: InventoryActions["onToggle"]
}) {
  const t = useTranslations("SkillMarketV2.inventory")
  const { agents } = useAcpAgents()
  const eligible = agents.filter(
    (agent) => agent.enabled && Boolean(agent.installed_version)
  )
  const blocked =
    skill.conflict ||
    skill.duplicate ||
    skill.observations.every((item) => item.readOnly)
  return (
    <section className="border-t pt-3">
      <h3 className="text-xs font-medium">{t("agents")}</h3>
      <div className="mt-2 divide-y border-y">
        {eligible.map((agent) => (
          <AgentToggle
            key={agent.agent_type}
            agentType={agent.agent_type}
            skill={skill}
            blocked={blocked}
            busyKey={busyKey}
            onToggle={onToggle}
          />
        ))}
        {!eligible.length ? (
          <p className="py-3 text-xs text-muted-foreground">{t("noAgents")}</p>
        ) : null}
      </div>
    </section>
  )
}
