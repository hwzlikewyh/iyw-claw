import type { LiveMessage } from "@/contexts/acp-connections-context"
import type { MessageTurn } from "@/lib/types"

export type TurnCompletion = "completed" | "interrupted" | "stopped"

export interface TurnCompletionRecord {
  id: string
  messageIds: string[]
  startedAt: number
  completedAt: number
  outcome: TurnCompletion
}

const MAX_COMPLETION_RECORDS = 100

export function completionFromStopReason(
  reason?: string
): TurnCompletion | null {
  if (!reason) return null
  if (reason === "end_turn") return "completed"
  if (reason === "cancelled") return "stopped"
  return "interrupted"
}

export function recordTurnCompletion(
  previous: TurnCompletionRecord[],
  message: LiveMessage | null,
  turns: MessageTurn[]
): TurnCompletionRecord[] {
  const outcome = completionFromStopReason(message?.stopReason)
  if (!message || !outcome) return previous
  let lastInput = turns.length - 1
  while (lastInput >= 0 && turns[lastInput].role !== "user") lastInput -= 1
  const replies = turns
    .slice(lastInput + 1)
    .filter((turn) => turn.role === "assistant")
  const inputTime = Date.parse(turns[lastInput]?.timestamp ?? "")
  const completedAt = message.completedAt ?? Date.now()
  const record = {
    id: message.id,
    messageIds: replies.map((turn) => turn.id),
    startedAt:
      inputTime >= message.startedAt && inputTime <= completedAt
        ? inputTime
        : message.startedAt,
    completedAt,
    outcome,
  }
  return mergeTurnCompletions(previous, [record])
}

export function mergeTurnCompletions(
  previous: TurnCompletionRecord[],
  incoming: TurnCompletionRecord[]
): TurnCompletionRecord[] {
  const records = new Map(
    [...previous, ...incoming].map((item) => [item.id, item])
  )
  return [...records.values()]
    .sort((left, right) => left.completedAt - right.completedAt)
    .slice(-MAX_COMPLETION_RECORDS)
}

export function recordedTurnCompletion(
  records: TurnCompletionRecord[] | undefined,
  turn: MessageTurn
): TurnCompletion | null {
  if (turn.role !== "assistant") return null
  // 历史落盘会替换 live ID，用后端时间定位同一回复；不跨轮次猜测。
  const timestamp = Date.parse(turn.completed_at ?? turn.timestamp)
  for (let index = (records?.length ?? 0) - 1; index >= 0; index -= 1) {
    const record = records![index]
    if (
      record.messageIds.includes(turn.id) ||
      (timestamp >= record.startedAt && timestamp <= record.completedAt)
    ) {
      return record.outcome
    }
  }
  return null
}
