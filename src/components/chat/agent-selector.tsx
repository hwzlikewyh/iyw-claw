"use client"

import { useEffect, useMemo, useRef } from "react"
import { useTranslations } from "next-intl"
import { Settings2 } from "lucide-react"
import { useAcpAgents } from "@/hooks/use-acp-agents"
import type { AgentType, AcpAgentInfo } from "@/lib/types"
import { getAgentDisplayName } from "@/lib/agent-sdk-presentation"
import { AgentIcon } from "@/components/agent-icon"
import { AgentSelectorSwitcher } from "./agent-selector-switcher"

type AgentSelectorVariant = "switcher" | "settings"

interface AgentSelectorProps {
  defaultAgentType?: AgentType
  onSelect: (agentType: AgentType) => void
  onFallback?: (agentType: AgentType) => void
  onAgentsLoaded?: (agents: AcpAgentInfo[]) => void
  onOpenAgentsSettings?: () => void
  disabled?: boolean
  variant?: AgentSelectorVariant
  align?: "start" | "center"
}

interface SelectionOptions {
  defaultAgentType?: AgentType
  onSelect: (agentType: AgentType) => void
  onFallback?: (agentType: AgentType) => void
  onAgentsLoaded?: (agents: AcpAgentInfo[]) => void
}

function useLatestRef<T>(value: T) {
  const ref = useRef(value)
  useEffect(() => {
    ref.current = value
  }, [value])
  return ref
}

function resolveSelected(
  agents: AcpAgentInfo[],
  requested?: AgentType
): AgentType | null {
  const requestedAgent = requested
    ? agents.find((agent) => agent.agent_type === requested && agent.available)
    : null
  return (
    requestedAgent?.agent_type ??
    agents.find((agent) => agent.available)?.agent_type ??
    null
  )
}

function useAgentSelection(options: SelectionOptions) {
  const { agents: rawAgents } = useAcpAgents()
  const agents = useMemo(
    () => rawAgents.filter((agent) => agent.enabled),
    [rawAgents]
  )
  const selected = useMemo(
    () => resolveSelected(agents, options.defaultAgentType),
    [agents, options.defaultAgentType]
  )
  const onSelectRef = useLatestRef(options.onSelect)
  const onFallbackRef = useLatestRef(options.onFallback)
  const onAgentsLoadedRef = useLatestRef(options.onAgentsLoaded)

  useEffect(() => {
    onAgentsLoadedRef.current?.(agents)
    if (options.defaultAgentType === selected || !selected) return
    const fallback = onFallbackRef.current
    if (fallback) fallback(selected)
    else onSelectRef.current(selected)
  }, [
    agents,
    onAgentsLoadedRef,
    onFallbackRef,
    onSelectRef,
    options.defaultAgentType,
    selected,
  ])

  return { agents, selected }
}

function EmptyAgentSelector({
  message,
  action,
  onOpenSettings,
}: {
  message: string
  action: string
  onOpenSettings?: () => void
}) {
  return (
    <div className="rounded-lg border border-dashed bg-muted/30 px-4 py-3 text-center text-sm text-muted-foreground">
      <div>{message}</div>
      {onOpenSettings ? (
        <button
          type="button"
          onClick={onOpenSettings}
          className="mt-2 inline-flex cursor-pointer items-center rounded-md border px-2 py-1 text-xs text-foreground transition-colors hover:bg-accent"
        >
          {action}
        </button>
      ) : null}
    </div>
  )
}

function SettingsAgentSelector({
  agent,
  title,
  onOpenSettings,
}: {
  agent: AcpAgentInfo
  title: string
  onOpenSettings?: () => void
}) {
  return (
    <div className="inline-flex items-center gap-1.5 rounded-full border border-border/60 bg-muted/40 px-2 py-1.5 text-xs text-muted-foreground">
      <AgentIcon agentType={agent.agent_type} className="h-4 w-4" />
      <span className="max-w-40 truncate text-foreground">
        {getAgentDisplayName(agent.agent_type)}
      </span>
      {onOpenSettings ? (
        <button
          type="button"
          onClick={onOpenSettings}
          className="inline-flex h-6 w-6 items-center justify-center rounded-full text-muted-foreground transition-colors hover:bg-accent hover:text-foreground"
          title={title}
          aria-label={title}
        >
          <Settings2 className="h-3.5 w-3.5" />
        </button>
      ) : null}
    </div>
  )
}

export function AgentSelector({
  defaultAgentType,
  onSelect,
  onFallback,
  onAgentsLoaded,
  onOpenAgentsSettings,
  disabled = false,
  variant = "switcher",
  align = "start",
}: AgentSelectorProps) {
  const t = useTranslations("Folder.chat.agentSelector")
  const { agents, selected } = useAgentSelection({
    defaultAgentType,
    onSelect,
    onFallback,
    onAgentsLoaded,
  })
  if (agents.length === 0) {
    return (
      <EmptyAgentSelector
        message={t("noEnabledAgents")}
        action={t("openAgentsSettings")}
        onOpenSettings={onOpenAgentsSettings}
      />
    )
  }
  if (variant === "settings") {
    const current =
      agents.find((agent) => agent.agent_type === selected) ?? agents[0]
    return (
      <SettingsAgentSelector
        agent={current}
        title={t("openAgentsSettings")}
        onOpenSettings={onOpenAgentsSettings}
      />
    )
  }
  return (
    <AgentSelectorSwitcher
      agents={agents}
      selected={selected}
      disabled={disabled}
      align={align}
      onSelect={onSelect}
      moreLabel={(count) => t("moreAgents", { count })}
    />
  )
}
