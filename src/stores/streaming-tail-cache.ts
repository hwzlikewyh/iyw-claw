import type {
  LiveContentBlock,
  LiveMessage,
} from "@/contexts/acp-connections-context"
import type { MessageTurn } from "@/lib/types"

interface StreamingTurns {
  turns: MessageTurn[]
  inProgressToolCallIds: Set<string>
}

interface CachedTail {
  message: LiveMessage
  result: StreamingTurns
}

// 稳定头块消失时整个缓存可释放，每个会话只保留当前版本。
const cache = new WeakMap<LiveContentBlock, Map<number, CachedTail>>()

export function rememberStreamingTail(
  conversationId: number,
  message: LiveMessage,
  result: StreamingTurns
): void {
  const head = message.content[0]
  if (!head) return
  let entries = cache.get(head)
  if (!entries) {
    entries = new Map()
    cache.set(head, entries)
  }
  entries.set(conversationId, { message, result })
}

export function reuseStreamingTail(
  conversationId: number,
  message: LiveMessage
): StreamingTurns | null {
  const head = message.content[0]
  const previous = head && cache.get(head)?.get(conversationId)
  if (!previous || !sameStablePrefix(previous.message, message)) return null
  const last = message.content[message.content.length - 1]
  const turn = previous.result.turns[previous.result.turns.length - 1]
  const oldBlock = turn?.blocks[turn.blocks.length - 1]
  if (
    !last ||
    !turn ||
    !oldBlock ||
    (last.type !== "text" && last.type !== "thinking")
  )
    return null
  if (oldBlock.type !== last.type || (last.type === "text" && !last.text))
    return null
  const result: StreamingTurns = {
    ...previous.result,
    turns: [
      ...previous.result.turns.slice(0, -1),
      {
        ...turn,
        blocks: [
          ...turn.blocks.slice(0, -1),
          { type: last.type, text: last.text },
        ],
      },
    ],
  }
  rememberStreamingTail(conversationId, message, result)
  return result
}

function sameStablePrefix(
  previous: LiveMessage,
  current: LiveMessage
): boolean {
  if (
    previous.id !== current.id ||
    previous.startedAt !== current.startedAt ||
    previous.recoveredVersion !== current.recoveredVersion ||
    previous.content.length !== current.content.length
  )
    return false
  const lastIndex = current.content.length - 1
  const last = previous.content[lastIndex]
  if (!last || (last.type !== "text" && last.type !== "thinking")) return false
  if (last.type === "text" && !last.text) return false
  for (let index = 0; index < lastIndex; index += 1) {
    if (previous.content[index] !== current.content[index]) return false
  }
  return last.type === current.content[lastIndex].type
}
