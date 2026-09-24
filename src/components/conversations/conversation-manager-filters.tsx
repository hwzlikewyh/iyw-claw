"use client"

import { useMemo } from "react"
import { useTranslations } from "next-intl"
import { Input } from "@/components/ui/input"
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select"
import { useAcpAgents } from "@/hooks/use-acp-agents"
import { getAgentDisplayName } from "@/lib/agent-sdk-presentation"
import {
  ALL_AGENT_TYPES,
  compareAgentType,
  STATUS_ORDER,
  isAgentType,
  type ConversationStatus,
} from "@/lib/types"
import type { ConversationManager } from "./use-conversation-manager"

export function ConversationManagerFilters({
  manager,
  scoped,
}: {
  manager: ConversationManager
  scoped: boolean
}) {
  const t = useTranslations("SidebarDesign")
  const tm = useTranslations("Folder.sidebar.manageConversations")
  const ts = useTranslations("Folder.statusLabels")
  const { agents } = useAcpAgents()
  const options = useMemo(
    () =>
      Array.from(
        new Set([
          ...ALL_AGENT_TYPES,
          ...agents.map((a) => a.agent_type),
          ...manager.rows.map((row) => row.agent_type),
          ...(manager.agent === "all" ? [] : [manager.agent]),
        ])
      ).sort(compareAgentType),
    [agents, manager.rows, manager.agent]
  )
  return (
    <div className="flex min-w-0 flex-wrap gap-2">
      <Input
        value={manager.search}
        disabled={manager.pending}
        onChange={(event) => manager.setSearch(event.target.value)}
        placeholder={tm("searchPlaceholder")}
        aria-label={tm("searchPlaceholder")}
        className="h-9 min-w-40 flex-1"
      />
      {!scoped && (
        <ManagerSelect
          label={t("project")}
          value={manager.project}
          disabled={manager.pending}
          onChange={manager.setProject}
          options={[
            { value: "all", label: t("allProjects") },
            ...manager.folders
              .filter((folder) => folder.kind !== "chat")
              .map((folder) => ({
                value: String(folder.id),
                label: folder.name,
              })),
          ]}
        />
      )}
      <ManagerSelect
        label={t("agent")}
        value={manager.agent}
        disabled={manager.pending}
        onChange={(value) => {
          if (value === "all" || isAgentType(value)) manager.setAgent(value)
        }}
        options={[
          { value: "all", label: tm("agentFilterAll") },
          ...options.map((agent) => ({
            value: agent,
            label: getAgentDisplayName(agent),
          })),
        ]}
      />
      <ManagerSelect
        label={t("status")}
        value={manager.status}
        disabled={manager.pending}
        onChange={(value) =>
          manager.setStatus(value as ConversationStatus | "all")
        }
        options={[
          { value: "all", label: tm("statusFilterAll") },
          ...STATUS_ORDER.map((status) => ({
            value: status,
            label: ts(status),
          })),
        ]}
      />
    </div>
  )
}

function ManagerSelect({
  label,
  value,
  disabled,
  onChange,
  options,
}: {
  label: string
  value: string
  disabled: boolean
  onChange: (value: string) => void
  options: { value: string; label: string }[]
}) {
  return (
    <Select value={value} disabled={disabled} onValueChange={onChange}>
      <SelectTrigger aria-label={label} className="h-9 w-36 min-w-0">
        <SelectValue />
      </SelectTrigger>
      <SelectContent>
        {options.map((option) => (
          <SelectItem key={option.value} value={option.value}>
            {option.label}
          </SelectItem>
        ))}
      </SelectContent>
    </Select>
  )
}
