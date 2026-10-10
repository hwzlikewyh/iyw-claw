import { getTransport, type Transport } from "./transport"
import {
  readConversationUsage,
  type ConversationUsageSnapshot,
} from "./conversation-usage-snapshot"

const EVENT_SETTLE_MS = 250
const MIN_REFRESH_MS = 1_000
const PERSIST_SETTLE_MS = 1_500
const RETRY_DELAY_MS = 3_000

type Listener = (snapshot: ConversationUsageSnapshot) => void
type Timer = ReturnType<typeof setTimeout>

interface Entry {
  transport: Transport
  conversationId: number
  listeners: Map<Listener, boolean>
  snapshot: ConversationUsageSnapshot | null
  pending: boolean
  dirty: boolean
  failed: boolean
  generation: number
  retryAvailable: boolean
  startedAt: number
  timer?: Timer
  settleTimer?: Timer
  cleanup: () => void
}

const entries = new WeakMap<Transport, Map<number, Entry>>()

function active(entry: Entry) {
  return entry.listeners.size > 0 && entry.transport === getTransport()
}

function schedule(entry: Entry, delay = EVENT_SETTLE_MS) {
  if (!active(entry) || document.hidden || entry.pending || entry.timer) return
  entry.timer = setTimeout(
    () => {
      entry.timer = undefined
      void refresh(entry)
    },
    Math.max(delay, entry.startedAt + MIN_REFRESH_MS - Date.now())
  )
}

function invalidate(entry: Entry, delay = EVENT_SETTLE_MS) {
  entry.dirty = true
  entry.retryAvailable = true
  schedule(entry, delay)
}

function retry(entry: Entry) {
  if (!entry.retryAvailable) return
  entry.retryAvailable = false
  entry.dirty = true
}

function publish(entry: Entry, snapshot: ConversationUsageSnapshot) {
  entry.snapshot = snapshot
  for (const listener of entry.listeners.keys()) listener(snapshot)
}

function current(entry: Entry, generation: number) {
  return active(entry) && !document.hidden && entry.generation === generation
}

function receive(
  entry: Entry,
  snapshot: ConversationUsageSnapshot,
  generation: number
) {
  if (!current(entry, generation)) return
  publish(entry, snapshot)
  const usage = snapshot.stats?.total_usage
  if (
    (snapshot.stats?.confirmed_consumption ?? usage?.confirmed_points)
      ?.state === "pending" ||
    (snapshot.stats?.confirmed_consumption ?? usage?.confirmed_points)
      ?.state === "partial"
  )
    retry(entry)
  if (!snapshot.turnUsage && [...entry.listeners.values()].some(Boolean))
    retry(entry)
  entry.failed = false
}

function failed(entry: Entry, error: unknown) {
  if (!entry.failed && active(entry)) {
    console.warn("[conversation-usage] refresh failed", {
      conversationId: entry.conversationId,
      error,
    })
  }
  retry(entry)
  entry.failed = true
}

async function refresh(entry: Entry) {
  if (!active(entry) || document.hidden || !entry.dirty || entry.pending) return
  entry.dirty = false
  entry.pending = true
  entry.startedAt = Date.now()
  const generation = entry.generation
  try {
    const snapshot = await readConversationUsage(
      entry.transport,
      {
        conversationId: entry.conversationId,
        includeTurn: [...entry.listeners.values()].some(Boolean),
      },
      () => current(entry, generation)
    )
    receive(entry, snapshot, generation)
  } catch (error) {
    failed(entry, error)
  } finally {
    entry.pending = false
    if (entry.dirty)
      schedule(entry, entry.retryAvailable ? EVENT_SETTLE_MS : RETRY_DELAY_MS)
    release(entry)
  }
}

function release(entry: Entry) {
  if (entry.listeners.size > 0) return
  clearTimeout(entry.timer)
  clearTimeout(entry.settleTimer)
  entry.timer = undefined
  entry.settleTimer = undefined
  entry.cleanup()
  entry.cleanup = () => {}
  entry.snapshot = null
  entry.generation += 1
  if (!entry.pending) entries.get(entry.transport)?.delete(entry.conversationId)
}

function listen(entry: Entry) {
  const visible = () => {
    if (document.hidden) {
      clearTimeout(entry.timer)
      entry.timer = undefined
    } else invalidate(entry)
  }
  document.addEventListener("visibilitychange", visible)
  const disconnect = entry.transport.onReconnect?.(() => invalidate(entry))
  entry.cleanup = () => {
    document.removeEventListener("visibilitychange", visible)
    disconnect?.()
  }
}

export function subscribeConversationUsage(
  conversationId: number,
  listener: Listener,
  includeTurn = false
): () => void {
  const transport = getTransport()
  let byConversation = entries.get(transport)
  if (!byConversation) entries.set(transport, (byConversation = new Map()))
  let entry = byConversation.get(conversationId)
  if (!entry) {
    entry = createEntry(transport, conversationId)
    byConversation.set(conversationId, entry)
  }
  const target = entry
  const first = target.listeners.size === 0
  const needsTurn = includeTurn && ![...target.listeners.values()].some(Boolean)
  target.listeners.set(listener, includeTurn)
  if (first) {
    listen(target)
    invalidate(target)
  } else if (needsTurn) {
    invalidate(target)
  } else if (target.snapshot) {
    queueMicrotask(() => {
      if (target.listeners.has(listener) && target.snapshot)
        listener(target.snapshot)
    })
  }
  return () => {
    target.listeners.delete(listener)
    release(target)
  }
}

function createEntry(transport: Transport, conversationId: number): Entry {
  return {
    transport,
    conversationId,
    listeners: new Map(),
    snapshot: null,
    pending: false,
    dirty: true,
    failed: false,
    generation: 0,
    retryAvailable: true,
    startedAt: 0,
    cleanup: () => {},
  }
}

export function invalidateConversationUsage(
  conversationId: number,
  options: { settled: boolean; reset: boolean }
) {
  const entry = entries.get(getTransport())?.get(conversationId)
  if (!entry) return
  if (options.reset) {
    entry.generation += 1
    entry.snapshot = null
    clearTimeout(entry.settleTimer)
    entry.settleTimer = undefined
  }
  invalidate(entry)
  if (!options.settled) return
  // 原生日志可能晚于回合终态落盘，只追加一次补读，不恢复周期轮询。
  clearTimeout(entry.settleTimer)
  entry.settleTimer = setTimeout(() => {
    entry.settleTimer = undefined
    invalidate(entry)
  }, PERSIST_SETTLE_MS)
}
