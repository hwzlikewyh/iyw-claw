"use client"

import { useTranslations } from "next-intl"

import { TaskArtifactFileRow } from "@/components/layout/task-artifact-file-row"
import { ScrollArea } from "@/components/ui/scroll-area"
import { getAgentDisplayName } from "@/lib/agent-sdk-presentation"
import type { TaskArtifactInfo } from "@/lib/api"

export interface TaskArtifactGroup {
  id: number
  title: string | null
  agentType: TaskArtifactInfo["agentType"] | null
  items: TaskArtifactInfo[]
}

interface TaskArtifactsListProps {
  groups: TaskArtifactGroup[]
  selectedId?: number
  openOnDoubleClick?: boolean
  onSelect: (item: TaskArtifactInfo) => void
  onOpenWorkspace?: () => void
}

export function TaskArtifactsList({
  groups,
  selectedId,
  openOnDoubleClick = false,
  onSelect,
  onOpenWorkspace,
}: TaskArtifactsListProps) {
  return (
    <ScrollArea className="min-h-0 flex-1">
      <div className="space-y-3 p-2">
        {groups.map((group) => (
          <ArtifactGroup
            key={group.id}
            group={group}
            selectedId={selectedId}
            openOnDoubleClick={openOnDoubleClick}
            onSelect={onSelect}
            onOpenWorkspace={onOpenWorkspace}
          />
        ))}
      </div>
    </ScrollArea>
  )
}

function ArtifactGroup({
  group,
  selectedId,
  openOnDoubleClick,
  onSelect,
  onOpenWorkspace,
}: Omit<TaskArtifactsListProps, "groups"> & { group: TaskArtifactGroup }) {
  return (
    <section className="space-y-1">
      {group.title !== null && <ArtifactGroupHeader group={group} />}
      {group.items.map((item) => (
        <TaskArtifactFileRow
          key={item.id}
          item={item}
          selected={item.id === selectedId}
          openOnDoubleClick={openOnDoubleClick}
          onSelect={onSelect}
          onOpenWorkspace={onOpenWorkspace}
        />
      ))}
    </section>
  )
}

function ArtifactGroupHeader({ group }: { group: TaskArtifactGroup }) {
  const t = useTranslations("Folder.taskArtifacts")
  return (
    <div className="flex min-w-0 items-center justify-between gap-2 px-1 text-xs text-muted-foreground">
      <span className="min-w-0 truncate">{group.title || t("untitled")}</span>
      <span className="shrink-0">
        {group.agentType ? getAgentDisplayName(group.agentType) : null}
        {group.agentType ? " · " : null}
        {group.items.length}
      </span>
    </div>
  )
}
