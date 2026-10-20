import { requestConversationHistoryPage } from "./conversation-history-request"
import { applyBackendConsumption } from "./backend-consumption"
import type { Transport } from "./transport"
import type { DbConversationDetail, SessionStats, TurnUsage } from "./types"
import { addUsagePoints } from "./usage-points"
import { mergeConfirmedPoints } from "./point-decimal"

export interface ConversationUsageSnapshot {
  ledgerExpected?: boolean
  backendConsumption?: DbConversationDetail["backend_consumption"]
  externalId: string | null
  stats: SessionStats | null
  turnUsage: TurnUsage | null
  inFlightUserTurnId: string | null
}

function mergeUsage(total: TurnUsage | null, usage: TurnUsage): TurnUsage {
  if (!total) return { ...usage }
  return {
    input_tokens: total.input_tokens + usage.input_tokens,
    output_tokens: total.output_tokens + usage.output_tokens,
    cache_read_input_tokens:
      total.cache_read_input_tokens + usage.cache_read_input_tokens,
    cache_creation_input_tokens:
      total.cache_creation_input_tokens + usage.cache_creation_input_tokens,
    estimated_points: addUsagePoints(total, usage),
    confirmed_points: mergeConfirmedPoints(
      total.confirmed_points,
      usage.confirmed_points
    ),
  }
}

function snapshotKey(detail: DbConversationDetail) {
  const usage = detail.session_stats?.total_usage
  return JSON.stringify([
    detail.summary.external_id,
    detail.in_flight_user_turn_id,
    detail.history_total_turns,
    detail.transcript_watermark,
    usage?.input_tokens,
    usage?.output_tokens,
    usage?.cache_read_input_tokens,
    usage?.cache_creation_input_tokens,
  ])
}

function withSnapshotConsumption(
  page: DbConversationDetail,
  first: DbConversationDetail
): DbConversationDetail {
  // 缓存里的金额可能属于旧账号，只应用本次首屏核对的权威账单。
  return applyBackendConsumption({
    ...page,
    turns: page.turns.map((turn) =>
      turn.usage
        ? { ...turn, usage: { ...turn.usage, confirmed_points: undefined } }
        : turn
    ),
    backend_consumption: first.backend_consumption,
  })
}

async function readTurnUsage(
  first: DbConversationDetail,
  readPage: (before: number) => Promise<DbConversationDetail>,
  isCurrent: () => boolean
): Promise<TurnUsage | null> {
  const boundary = first.in_flight_user_turn_id
  if (boundary == null) return null
  const revision = snapshotKey(first)
  let page = first
  let usage: TurnUsage | null = null
  while (isCurrent()) {
    for (let index = page.turns.length - 1; index >= 0; index -= 1) {
      const turn = page.turns[index]
      if (turn.id === boundary) {
        return usage
      }
      if (turn.role === "assistant" && turn.usage) {
        usage = mergeUsage(usage, turn.usage)
      }
    }
    if (page.history_start === 0) break
    const before = page.history_start
    page = await readPage(before)
    if (page.history_start >= before || snapshotKey(page) !== revision) break
    page = withSnapshotConsumption(page, first)
  }
  // 边界缺失或翻页期间快照变化时不把不完整结果当成本轮消费。
  return null
}

export async function readConversationUsage(
  transport: Transport,
  request: { conversationId: number; includeTurn: boolean },
  isCurrent: () => boolean
): Promise<ConversationUsageSnapshot> {
  const readPage = (before?: number) =>
    requestConversationHistoryPage(transport, {
      conversationId: request.conversationId,
      before,
      // 首屏核对实时用量；更早的页复用同一快照，避免每翻一页重解析整段历史。
      forceRefresh: before === undefined,
    })
  const first = await readPage()
  const turnUsage =
    request.includeTurn && first.in_flight_user_turn_id != null
      ? await readTurnUsage(first, readPage, isCurrent)
      : null
  // 只保留用量，完整正文和工具输出在本次读取结束后释放。
  return {
    ledgerExpected:
      first.summary.agent_type === "codex" ||
      first.summary.agent_type === "claude_code",
    backendConsumption: first.backend_consumption,
    externalId: first.summary.external_id,
    stats: first.session_stats ?? null,
    turnUsage,
    inFlightUserTurnId: first.in_flight_user_turn_id ?? null,
  }
}
