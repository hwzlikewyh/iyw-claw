import type { TaskArtifactInfo, TaskArtifactPage } from "./api"
import type { Transport } from "./transport"

const MAX_CONCURRENT_REQUESTS = 2

interface ArtifactRequest {
  conversationId: number | null
  messageId: string | null
  folderId: number | null
  latestTurnOnly: boolean
  search: string | null
  page: number
  pageSize: number
}

interface RequestQueue {
  active: number
  waiting: Array<() => void>
  pending: Map<string, Promise<TaskArtifactPage>>
}

const queues = new WeakMap<Transport, RequestQueue>()

export function invalidateTaskArtifactRequests(transport: Transport): void {
  // 实时事件之后的读取不能复用事件之前已经发出的旧快照。
  queues.get(transport)?.pending.clear()
}

export function requestTaskArtifacts(
  transport: Transport,
  request: ArtifactRequest
): Promise<TaskArtifactPage> {
  let queue = queues.get(transport)
  if (!queue) {
    queue = { active: 0, waiting: [], pending: new Map() }
    queues.set(transport, queue)
  }
  const key = JSON.stringify(request)
  const existing = queue.pending.get(key)
  if (existing) return existing

  const pending = loadArtifacts(transport, request, queue).finally(() => {
    if (queue.pending.get(key) === pending) queue.pending.delete(key)
  })
  queue.pending.set(key, pending)
  return pending
}

async function loadArtifacts(
  transport: Transport,
  request: ArtifactRequest,
  queue: RequestQueue
): Promise<TaskArtifactPage> {
  if (queue.active >= MAX_CONCURRENT_REQUESTS) {
    await new Promise<void>((resolve) => queue.waiting.push(resolve))
  } else {
    queue.active += 1
  }
  try {
    const result = await transport.call<TaskArtifactPage | TaskArtifactInfo[]>(
      "list_task_artifacts",
      { ...request }
    )
    if (!Array.isArray(result)) return result
    return {
      items: result,
      total: result.length,
      page: request.page,
      pageSize: request.pageSize,
    }
  } finally {
    // 将名额直接交给下一个请求，避免新请求插队或短暂超出并发上限。
    const next = queue.waiting.shift()
    if (next) next()
    else queue.active -= 1
  }
}
