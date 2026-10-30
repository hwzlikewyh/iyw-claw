import type { Transport } from "./transport"
import type { AgentInputItem, DbConversationDetail } from "./types"
import { mergeAgentInputHistory } from "./agent-input-history"
import { applyBackendConsumption } from "./backend-consumption"
import { consumptionEpoch } from "@/stores/conversation-consumption"

interface HistoryRequest {
  conversationId: number
  before?: number
  forceRefresh: boolean
}

interface HistoryBatch {
  pending: Promise<DbConversationDetail>
  queued?: Promise<DbConversationDetail>
  joinable: boolean
}

const batches = new WeakMap<Transport, Map<string, HistoryBatch>>()
const inputBatches = new WeakMap<
  Transport,
  Map<string, Promise<AgentInputItem[]>>
>()

export async function requestConversationHistory(
  transport: Transport,
  request: HistoryRequest
): Promise<DbConversationDetail> {
  if (request.before !== undefined)
    return requestConversationHistoryPage(transport, request)
  const [detail, inputs] = await Promise.all([
    requestConversationHistoryPage(transport, request),
    requestAgentInputs(transport, request.conversationId),
  ])
  return mergeAgentInputHistory(detail, inputs)
}

function requestAgentInputs(
  transport: Transport,
  conversationId: number
): Promise<AgentInputItem[]> {
  let batch = inputBatches.get(transport)
  if (!batch) {
    batch = new Map()
    inputBatches.set(transport, batch)
  }
  const key = JSON.stringify([consumptionEpoch(), conversationId])
  const existing = batch.get(key)
  if (existing) return existing
  const pending = transport.call<AgentInputItem[]>("list_agent_inputs", {
    conversationId,
  })
  batch.set(key, pending)
  const clear = () => {
    if (batch.get(key) === pending) batch.delete(key)
  }
  void pending.then(clear, clear)
  return pending
}

// 用量读取只需要历史页，不加载和合并可能包含大附件的输入队列。
export function requestConversationHistoryPage(
  transport: Transport,
  request: HistoryRequest
): Promise<DbConversationDetail> {
  let batch = batches.get(transport)
  if (!batch) {
    batch = new Map()
    batches.set(transport, batch)
  }
  const key = JSON.stringify([
    consumptionEpoch(),
    request.conversationId,
    request.before ?? null,
    request.forceRefresh,
  ])
  const load = () => loadHistory(transport, request)
  const existing = batch.get(key)
  if (existing) {
    if (!request.forceRefresh || existing.joinable) return existing.pending
    // 在途读取后收到的新状态合并为一次补读，不能复用旧快照吞掉更新。
    existing.queued ??= existing.pending.then(
      () => startHistoryBatch(batch, key, load),
      () => startHistoryBatch(batch, key, load)
    )
    return existing.queued
  }
  return startHistoryBatch(batch, key, load)
}

function startHistoryBatch(
  batch: Map<string, HistoryBatch>,
  key: string,
  load: () => Promise<DbConversationDetail>
): Promise<DbConversationDetail> {
  const pending = load()
  const entry: HistoryBatch = { pending, joinable: true }
  batch.set(key, entry)
  const clear = () => {
    if (batch.get(key) === entry && !entry.queued) batch.delete(key)
  }
  queueMicrotask(() => {
    entry.joinable = false
  })
  void pending.then(clear, clear)
  return pending
}

async function loadHistory(
  transport: Transport,
  request: HistoryRequest
): Promise<DbConversationDetail> {
  const { conversationId, before, forceRefresh } = request
  const detail = await transport.call<DbConversationDetail>(
    "get_folder_conversation",
    { conversationId, before: before ?? null, forceRefresh }
  )
  return applyBackendConsumption(detail)
}
