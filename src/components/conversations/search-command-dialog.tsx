"use client"

import { useState } from "react"
import { Folder } from "lucide-react"
import { useTranslations } from "next-intl"
import type { AgentType } from "@/lib/types"
import type { TaskArtifactInfo } from "@/lib/api"
import { useWorkbenchRoute } from "@/contexts/workbench-route-context"
import { getAgentDisplayName } from "@/lib/agent-sdk-presentation"
import { AgentIcon } from "@/components/agent-icon"
import { TaskArtifactDialog } from "@/components/layout/task-artifact-dialog"
import {
  CommandDialog,
  CommandInput,
  CommandList,
} from "@/components/ui/command"
import { cn } from "@/lib/utils"
import { useCommandSearchScope } from "./use-command-search"
import { ConversationSearchResults } from "./search-conversation-results"
import { FileSearchResults } from "./search-file-results"

type SearchTab = "conversations" | "files"
interface SearchCommandDialogProps {
  open: boolean
  onOpenChange: (open: boolean) => void
}
interface SearchContentProps {
  scope: ReturnType<typeof useCommandSearchScope>
  onClose: () => void
  onSelectArtifact: (item: TaskArtifactInfo) => void
}

export function SearchCommandDialog({
  open,
  onOpenChange,
}: SearchCommandDialogProps) {
  const t = useTranslations("Folder.search")
  const scope = useCommandSearchScope()
  const { openConversations } = useWorkbenchRoute()
  const [artifact, setArtifact] = useState<TaskArtifactInfo | null>(null)
  const title = scope.folder
    ? t("dialogTitleWithFolder", { name: scope.folder.name })
    : t("dialogTitle")
  return (
    <>
      <CommandDialog
        title={title}
        open={open}
        onOpenChange={onOpenChange}
        shouldFilter={false}
      >
        {open && (
          <SearchContent
            key={scope.folderId ?? "workspace"}
            scope={scope}
            onClose={() => onOpenChange(false)}
            onSelectArtifact={(item) => {
              openConversations()
              setArtifact(item)
              onOpenChange(false)
            }}
          />
        )}
      </CommandDialog>
      <TaskArtifactDialog
        artifact={artifact}
        open={artifact !== null}
        onOpenChange={(next) => {
          if (!next) setArtifact(null)
        }}
      />
    </>
  )
}

function SearchContent({
  scope,
  onClose,
  onSelectArtifact,
}: SearchContentProps) {
  const t = useTranslations("Folder.search")
  const [tab, setTab] = useState<SearchTab>("conversations")
  const [query, setQuery] = useState("")
  const [agent, setAgent] = useState<AgentType | null>(null)
  return (
    <>
      <SearchHeader scope={scope} />
      <SearchTabs tab={tab} onChange={setTab} />
      <CommandInput
        placeholder={
          tab === "conversations" ? t("placeholder") : t("filePlaceholder")
        }
        value={query}
        onValueChange={setQuery}
      />
      {tab === "conversations" && (
        <SearchAgentFilter
          agents={scope.agents}
          value={agent}
          onChange={setAgent}
        />
      )}
      <CommandList className="h-[min(24rem,50dvh)] max-h-[50dvh]">
        {tab === "conversations" ? (
          <ConversationSearchResults
            key={JSON.stringify([query.trim(), scope.folderId, agent])}
            query={query}
            folderId={scope.folderId}
            agent={agent}
            onClose={onClose}
          />
        ) : (
          <FileSearchResults
            query={query}
            folderId={scope.folderId}
            folderPath={scope.folder?.path}
            onClose={onClose}
            onSelectArtifact={onSelectArtifact}
          />
        )}
      </CommandList>
    </>
  )
}

function SearchHeader({ scope }: Pick<SearchContentProps, "scope">) {
  const t = useTranslations("Folder.search")
  return (
    <div className="flex items-center gap-2 border-b px-4 py-2.5 pr-10">
      <Folder className="size-4 shrink-0 text-muted-foreground" />
      <span className="truncate text-sm font-medium">
        {scope.folder
          ? t("dialogTitleWithFolder", { name: scope.folder.name })
          : t("dialogTitle")}
      </span>
    </div>
  )
}

function SearchTabs({
  tab,
  onChange,
}: {
  tab: SearchTab
  onChange: (tab: SearchTab) => void
}) {
  const t = useTranslations("Folder.search")
  return (
    <div className="flex items-center border-b px-3" role="tablist">
      {(["conversations", "files"] as const).map((value) => (
        <button
          key={value}
          type="button"
          role="tab"
          aria-selected={tab === value}
          onClick={() => onChange(value)}
          className={cn(
            "relative h-9 px-3 text-sm font-medium transition-colors",
            tab === value
              ? "text-foreground"
              : "text-muted-foreground hover:text-foreground"
          )}
        >
          {t(value === "conversations" ? "tabConversations" : "tabFiles")}
          {tab === value && (
            <span className="absolute bottom-0 left-3 right-3 h-0.5 rounded-full bg-foreground" />
          )}
        </button>
      ))}
    </div>
  )
}

function SearchAgentFilter({
  agents,
  value,
  onChange,
}: {
  agents: AgentType[]
  value: AgentType | null
  onChange: (agent: AgentType | null) => void
}) {
  const t = useTranslations("Folder.search")
  if (agents.length < 2) return null
  return (
    <div className="flex flex-wrap items-center gap-1 border-b px-3 py-2">
      {[null, ...agents].map((agent) => (
        <button
          key={agent ?? "all"}
          type="button"
          aria-pressed={agent === value}
          onClick={() => onChange(agent)}
          className={cn(
            "flex h-6 shrink-0 items-center gap-1.5 rounded-md px-2 text-xs transition-colors",
            agent === value
              ? "bg-secondary text-secondary-foreground"
              : "text-muted-foreground hover:text-foreground"
          )}
        >
          {agent && <AgentIcon agentType={agent} className="size-3.5" />}
          {agent ? getAgentDisplayName(agent) : t("allAgents")}
        </button>
      ))}
    </div>
  )
}
