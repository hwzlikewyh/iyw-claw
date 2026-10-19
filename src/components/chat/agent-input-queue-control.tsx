"use client"

import { useTranslations } from "next-intl"
import { Clock3 } from "lucide-react"
import {
  Popover,
  PopoverContent,
  PopoverTrigger,
} from "@/components/ui/popover"
import { AgentInputWaitingDisplay } from "./agent-input-waiting-display"
import { useAgentInputDelivery } from "./agent-input-delivery-context"

export function AgentInputQueueControl() {
  const state = useAgentInputDelivery()
  const t = useTranslations("ConversationPresentation")
  if (!state?.pending.length) return null
  return (
    <div className="mx-auto flex w-full max-w-4xl justify-end px-4 pb-1">
      <Popover>
        <PopoverTrigger asChild>
          <button type="button" className="conversation-task-trigger">
            <Clock3 aria-hidden="true" className="size-3" />
            {t("pendingInputs", { count: state.pending.length })}
          </button>
        </PopoverTrigger>
        <PopoverContent
          side="top"
          align="end"
          className="w-96 max-w-[calc(100vw-2rem)] gap-3 p-3"
        >
          <strong className="text-xs">
            {t("pendingInputs", { count: state.pending.length })}
          </strong>
          <AgentInputWaitingDisplay
            items={state.pending}
            onDelete={state.online ? state.onDelete : undefined}
            onRetry={state.online ? state.onRetry : undefined}
            onReorder={state.online ? state.onReorder : undefined}
            onForceThrough={state.online ? state.onForceThrough : undefined}
          />
        </PopoverContent>
      </Popover>
    </div>
  )
}
