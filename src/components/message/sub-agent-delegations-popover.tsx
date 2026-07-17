"use client"

import { memo, useState } from "react"
import { useTranslations } from "next-intl"
import { BotIcon } from "lucide-react"

import { AgentIcon } from "@/components/agent-icon"
import { Badge } from "@/components/ui/badge"
import {
  Popover,
  PopoverContent,
  PopoverTrigger,
} from "@/components/ui/popover"
import { StatusBadge } from "@/components/message/delegation-status-badge"
import { SubAgentSessionDialog } from "@/components/message/sub-agent-session-dialog"
import {
  useDelegationCardModel,
  type DelegationCardSource,
} from "@/hooks/use-delegation-card-model"
import { getAgentDisplayName } from "@/lib/agent-sdk-presentation"

interface SubAgentDelegationsPopoverProps {
  delegations: DelegationCardSource[]
}

export const SubAgentDelegationsPopover = memo(
  function SubAgentDelegationsPopover({
    delegations,
  }: SubAgentDelegationsPopoverProps) {
    const t = useTranslations("Folder.chat.subAgentOverlay")
    const count = delegations.length

    if (count === 0) return null

    return (
      <Popover>
        <PopoverTrigger asChild>
          <button
            type="button"
            className="inline-flex h-5 items-center gap-1 rounded-full px-1.5 leading-none text-muted-foreground transition-colors hover:bg-accent hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
          >
            <BotIcon className="h-3 w-3 shrink-0" />
            <span>{t("title")}</span>
            <Badge
              variant="secondary"
              className="h-4 px-1 text-[10px] leading-none"
            >
              {count}
            </Badge>
          </button>
        </PopoverTrigger>
        <PopoverContent
          side="top"
          align="center"
          className="w-80 max-w-[calc(100vw-2rem)] gap-0 overflow-hidden p-0"
        >
          <div className="flex items-center gap-2 border-b px-3 py-2">
            <BotIcon className="h-4 w-4 shrink-0 text-muted-foreground" />
            <span className="min-w-0 flex-1 truncate text-sm font-medium">
              {t("title")}
            </span>
            <Badge variant="secondary" className="h-5 shrink-0">
              {count}
            </Badge>
          </div>
          <div className="max-h-72 space-y-1.5 overflow-y-auto p-2">
            {delegations.map((source) => (
              <SubAgentDelegationRow
                key={source.parentToolUseId}
                source={source}
              />
            ))}
          </div>
        </PopoverContent>
      </Popover>
    )
  }
)

export const SubAgentDelegationRow = memo(function SubAgentDelegationRow({
  source,
}: {
  source: DelegationCardSource
}) {
  const t = useTranslations("Folder.chat.delegation")
  const [dialogOpen, setDialogOpen] = useState(false)
  const {
    agentType,
    task,
    taskId,
    status,
    errorCode,
    childConversationId,
    childConnectionId,
  } = useDelegationCardModel(source)

  const clickable = childConversationId != null

  const rowBody = (
    <div className="min-w-0 flex-1 space-y-1">
      <div className="flex items-center gap-1.5">
        <span className="inline-flex h-5 w-5 shrink-0 items-center justify-center rounded-full border border-border bg-background text-foreground">
          {agentType ? (
            <AgentIcon agentType={agentType} className="h-3.5 w-3.5" />
          ) : (
            <span className="h-1.5 w-1.5 rounded-sm bg-muted-foreground/60" />
          )}
        </span>
        <span className="min-w-0 truncate text-xs font-semibold text-foreground">
          {agentType ? getAgentDisplayName(agentType) : t("unknownAgent")}
        </span>
        {taskId && (
          <span
            className="shrink-0 font-mono text-[11px] text-muted-foreground"
            title={taskId}
          >
            #{taskId.slice(0, 8)}
          </span>
        )}
        <StatusBadge status={status} errorCode={errorCode} />
      </div>
      {task && (
        <div className="truncate text-[11px] text-muted-foreground">{task}</div>
      )}
    </div>
  )

  return (
    <>
      {clickable ? (
        <button
          type="button"
          data-testid="sub-agent-row"
          onClick={() => setDialogOpen(true)}
          className="flex w-full items-center gap-2 rounded-lg border bg-transparent px-2 py-1.5 text-left transition-colors hover:bg-muted/60"
          title={t("openDetail")}
        >
          {rowBody}
        </button>
      ) : (
        <div
          data-testid="sub-agent-row"
          className="flex w-full items-center gap-2 rounded-lg border bg-transparent px-2 py-1.5"
        >
          {rowBody}
        </div>
      )}
      {childConversationId != null && (
        <SubAgentSessionDialog
          open={dialogOpen}
          onOpenChange={setDialogOpen}
          childConversationId={childConversationId}
          childConnectionId={childConnectionId}
          agentType={agentType}
          kickoffTask={task}
        />
      )}
    </>
  )
})
