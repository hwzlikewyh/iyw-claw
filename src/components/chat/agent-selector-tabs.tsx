"use client"

import type { AcpAgentInfo, AgentType } from "@/lib/types"
import { getAgentDisplayName } from "@/lib/agent-sdk-presentation"
import { cn } from "@/lib/utils"

export function AgentSelectorTabs({
  agents,
  selected,
  disabled,
  onSelect,
}: {
  agents: AcpAgentInfo[]
  selected: AgentType | null
  disabled: boolean
  onSelect: (agentType: AgentType) => void
}) {
  return (
    <div
      data-slot="agent-selector"
      className="flex min-w-0 max-w-full overflow-x-auto"
    >
      <div className="inline-flex h-8 items-center rounded-lg bg-[#f8fafc] p-0.5 dark:bg-muted">
        {agents.map((agent) => (
          <button
            key={agent.agent_type}
            type="button"
            data-slot="agent-tab"
            aria-pressed={selected === agent.agent_type}
            disabled={disabled || !agent.available}
            onClick={() => onSelect(agent.agent_type)}
            className={cn(
              "h-7 shrink-0 cursor-pointer rounded-md px-3 text-[13px] transition-colors focus-visible:outline-2 focus-visible:outline-ring disabled:cursor-not-allowed disabled:opacity-40",
              selected === agent.agent_type
                ? "bg-white text-[#111827] dark:bg-background dark:text-foreground"
                : "text-[#94a3b8] hover:text-[#111827] dark:text-muted-foreground dark:hover:text-foreground"
            )}
          >
            {getAgentDisplayName(agent.agent_type)}
          </button>
        ))}
      </div>
    </div>
  )
}
