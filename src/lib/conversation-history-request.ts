import type { Transport } from "./transport"
import type { AgentInputItem, DbConversationDetail } from "./types"
import { mergeAgentInputHistory } from "./agent-input-history"

interface HistoryRequest {
  conversationId: number
  before?: number
  forceRefresh: boolean
}

const batches = new WeakMap<
  Transport,
  Map<string, Promise<DbConversationDetail>>
>()

export function requestConversationHistory(
  transport: Transport,
  request: HistoryRequest
): Promise<DbConversationDetail> {
  let batch = batches.get(transport)
  if (!batch) {
    batch = new Map()
    batches.set(transport, batch)
  }
  const key = JSON.stringify([
    request.conversationId,
    request.before ?? null,
    request.forceRefresh,
  ])
  const existing = batch.get(key)
  if (existing) return existing
  const pending = loadHistory(transport, request)
  batch.set(key, pending)
  const clear = () => {
    if (batch.get(key) === pending) batch.delete(key)
  }
  // 实时事件的强制刷新必须取新状态；普通历史加载复用仍在进行的请求。
  if (request.forceRefresh) queueMicrotask(clear)
  else void pending.then(clear, clear)
  return pending
}

async function loadHistory(
  transport: Transport,
  request: HistoryRequest
): Promise<DbConversationDetail> {
  const { conversationId, before, forceRefresh } = request
  const [detail, inputs] = await Promise.all([
    transport.call<DbConversationDetail>("get_folder_conversation", {
      conversationId,
      before: before ?? null,
      forceRefresh,
    }),
    before === undefined
      ? transport.call<AgentInputItem[]>("list_agent_inputs", {
          conversationId,
        })
      : Promise.resolve([]),
  ])
  return mergeAgentInputHistory(detail, inputs)
}
