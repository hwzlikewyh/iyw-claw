"use client"

import { AgentInputWaitingDisplay } from "./agent-input-waiting-display"
import { useAgentInputDelivery } from "./agent-input-delivery-context"

export function AgentInputQueueControl() {
  const state = useAgentInputDelivery()
  if (!state?.pending.length) return null
  return (
    <div className="mx-auto w-full max-w-4xl px-4">
      <AgentInputWaitingDisplay
        items={state.pending}
        onDelete={state.online ? state.onDelete : undefined}
        onRetry={state.online ? state.onRetry : undefined}
        onReorder={state.online ? state.onReorder : undefined}
        onForceThrough={state.online ? state.onForceThrough : undefined}
        onSendNow={state.online ? state.onSendNow : undefined}
      />
    </div>
  )
}
