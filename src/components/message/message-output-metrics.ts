import type { ContentBlock, MessageTurn } from "@/lib/types"

const characterCounts = new WeakMap<ContentBlock, number>()
const turnMetrics = new WeakMap<
  MessageTurn,
  ReturnType<typeof calculateMetrics>
>()

function textCharacters(block: ContentBlock): number {
  if (block.type !== "text") return 0
  const cached = characterCounts.get(block)
  if (cached !== undefined) return cached
  // 保持 Unicode 码点口径；普通中英文直接使用长度，合并代理对。
  const pairs = block.text.match(/[\uD800-\uDBFF][\uDC00-\uDFFF]/g)
  const count = block.text.length - (pairs?.length ?? 0)
  characterCounts.set(block, count)
  return count
}

function calculateMetrics(turn: MessageTurn) {
  let characters = 0
  let tools = 0
  const toolIds = new Set<string>()
  for (const block of turn.blocks) {
    characters += textCharacters(block)
    if (block.type === "image_generation") tools += 1
    if (block.type !== "tool_use") continue
    if (block.tool_use_id && toolIds.has(block.tool_use_id)) continue
    if (block.tool_use_id) toolIds.add(block.tool_use_id)
    tools += 1
  }
  return { characters, tools }
}

export function getMessageOutputMetrics(turn: MessageTurn) {
  const cached = turnMetrics.get(turn)
  if (cached) return cached
  const metrics = calculateMetrics(turn)
  turnMetrics.set(turn, metrics)
  return metrics
}
