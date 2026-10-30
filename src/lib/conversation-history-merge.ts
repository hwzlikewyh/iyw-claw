import type { DbConversationDetail } from "./types"
import { applyBackendConsumption } from "./backend-consumption"

function sameConversation(a: DbConversationDetail, b: DbConversationDetail) {
  return (
    a.summary.id === b.summary.id &&
    a.summary.external_id === b.summary.external_id
  )
}

// 尾页刷新只替换重叠后的内容，保留用户已经展开的历史前缀。
export function mergeRefreshedConversationHistory(
  current: DbConversationDetail | null,
  incoming: DbConversationDetail
): DbConversationDetail {
  if (
    !current ||
    !sameConversation(current, incoming) ||
    current.history_start >= incoming.history_start ||
    current.history_total_turns > incoming.history_total_turns
  ) {
    return incoming
  }
  const first = incoming.turns[0]
  if (!first) return incoming
  const boundary = current.turns.findIndex(
    (turn) => turn.id === first.id && turn.role === first.role
  )
  // 没有重叠时无法证明两页连续，交由最新快照替换，避免拼出缺页历史。
  if (boundary <= 0) return incoming
  return applyBackendConsumption({
    ...incoming,
    turns: [...current.turns.slice(0, boundary), ...incoming.turns],
    history_start: current.history_start,
    history_assistant_turns_before: current.history_assistant_turns_before,
  })
}

export function mergeEarlierConversationHistory(
  current: DbConversationDetail,
  incoming: DbConversationDetail
): DbConversationDetail {
  if (!sameConversation(current, incoming)) return current
  // 只核对新增页的账单；保留已渲染轮次的引用，避免每次翻页重建全部正文。
  incoming = applyBackendConsumption({
    ...incoming,
    backend_consumption:
      current.backend_consumption ?? incoming.backend_consumption,
  })
  const key = (turn: DbConversationDetail["turns"][number]) =>
    `${turn.role} ${turn.id}`
  const known = new Set(current.turns.map(key))
  const refreshed = new Map(incoming.turns.map((turn) => [key(turn), turn]))
  const older = incoming.turns.filter((turn) => !known.has(key(turn)))
  return {
    ...current,
    // 仅复用当前登录身份核对过的账单，缓存页中的金额已在读取时撤回。
    backend_consumption:
      current.backend_consumption ?? incoming.backend_consumption,
    turns: [
      ...older,
      ...current.turns.map((turn) => refreshed.get(key(turn)) ?? turn),
    ],
    history_start: Math.min(current.history_start, incoming.history_start),
    history_total_turns: Math.max(
      current.history_total_turns,
      incoming.history_total_turns
    ),
    history_assistant_turns_before:
      incoming.history_start <= current.history_start
        ? incoming.history_assistant_turns_before
        : current.history_assistant_turns_before,
  }
}
