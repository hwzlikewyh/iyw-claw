"use client"

import { ArrowUp, Check, CircleAlert, Clock3, X } from "lucide-react"
import { useTranslations } from "next-intl"
import {
  useAgentInputDelivery,
  inputDeliveryKey,
} from "@/components/chat/agent-input-delivery-context"
import { WaitingActions } from "@/components/chat/agent-input-waiting-row"
import { cn } from "@/lib/utils"

const DELIVERY_ICONS = {
  waiting: Clock3,
  dispatching: ArrowUp,
  fallback_queued: Clock3,
  consumed: Check,
  failed: CircleAlert,
  deleted: X,
}

export function AgentInputDelivery({ inputId }: { inputId: string }) {
  const state = useAgentInputDelivery()
  const t = useTranslations("ConversationPresentation")
  const item = state?.byId.get(inputId)
  if (!state || !item) return null
  const Icon = DELIVERY_ICONS[item.status]
  const pending = state.pending.some((entry) => entry.id === item.id)
  return (
    <div
      className={cn(
        "conversation-input-delivery flex min-w-0 items-center justify-end gap-1.5 text-[11px] text-muted-foreground",
        item.status === "failed" && "text-destructive"
      )}
    >
      <Icon aria-hidden="true" className="size-3 shrink-0" />
      <span role="status" aria-live="polite" aria-atomic="true">
        {t(inputDeliveryKey(item, state.online))}
      </span>
      {state.online && pending && (
        <WaitingActions
          item={item}
          index={state.pending.findIndex((entry) => entry.id === item.id)}
          visible={state.pending}
          onDelete={state.onDelete}
          onRetry={state.onRetry}
          onForceThrough={state.onForceThrough}
          reorderDisabled
          forceDisabled={state.pending.some(
            (entry) =>
              entry.force_batch_id != null || entry.force_requested_at != null
          )}
        />
      )}
    </div>
  )
}
