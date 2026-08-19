"use client"

import { useEffect, useMemo, useRef } from "react"
import { Bot, Loader2 } from "lucide-react"
import { useTranslations } from "next-intl"
import { Checkbox } from "@/components/ui/checkbox"
import { useAcpAgents } from "@/hooks/use-acp-agents"
import { getAgentLabel } from "@/lib/custom-agents"
import type { AgentType } from "@/lib/types"

export function AgentTargets({
  selected,
  onChange,
}: {
  selected: Set<AgentType>
  onChange: (next: Set<AgentType>) => void
}) {
  const t = useTranslations("SkillMarketV2")
  const { agents, fresh } = useAcpAgents()
  const eligible = useMemo(
    () => agents.filter((agent) => agent.enabled && agent.installed_version),
    [agents]
  )
  const initializedRef = useRef(false)

  useEffect(() => {
    if (!fresh || initializedRef.current) return
    initializedRef.current = true
    onChange(new Set(eligible.map((agent) => agent.agent_type)))
  }, [eligible, fresh, onChange])

  if (!fresh) {
    return (
      <div className="flex h-20 items-center justify-center text-xs text-muted-foreground">
        <Loader2 className="mr-2 size-3.5 animate-spin" aria-hidden="true" />
        {t("install.targetsLoading")}
      </div>
    )
  }
  if (!eligible.length) {
    return (
      <div className="flex min-h-20 items-center gap-3 border-y py-3 text-xs text-muted-foreground">
        <Bot className="size-4 shrink-0" aria-hidden="true" />
        {t("install.noTargets")}
      </div>
    )
  }
  return (
    <div className="divide-y border-y">
      {eligible.map((agent) => (
        <AgentTargetRow
          key={agent.agent_type}
          agentType={agent.agent_type}
          version={agent.installed_version ?? "-"}
          checked={selected.has(agent.agent_type)}
          onChange={(checked) => {
            const next = new Set(selected)
            if (checked) next.add(agent.agent_type)
            else next.delete(agent.agent_type)
            onChange(next)
          }}
        />
      ))}
    </div>
  )
}

function AgentTargetRow({
  agentType,
  version,
  checked,
  onChange,
}: {
  agentType: AgentType
  version: string
  checked: boolean
  onChange: (checked: boolean) => void
}) {
  const t = useTranslations("SkillMarketV2")
  return (
    <label className="flex min-h-12 cursor-pointer items-center gap-3 py-2">
      <Checkbox
        checked={checked}
        onCheckedChange={(value) => onChange(value === true)}
      />
      <span className="min-w-0 flex-1">
        <span className="block text-xs font-medium">
          {getAgentLabel(agentType)}
        </span>
        <span className="block truncate text-[10px] text-muted-foreground">
          {t("install.targetVersion", { version })}
        </span>
      </span>
      <span className="text-[10px] text-muted-foreground">
        {t("install.defaultMode")}
      </span>
    </label>
  )
}
