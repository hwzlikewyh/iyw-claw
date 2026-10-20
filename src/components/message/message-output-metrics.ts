import type { MessageTurn } from "@/lib/types"
import type { AdaptedContentPart } from "@/lib/adapters/ai-elements-adapter"
import { splitAssistantTurnParts } from "./assistant-turn-model"

const characterCounts = new WeakMap<AdaptedContentPart[], number>()
const turnMetrics = new WeakMap<
  MessageTurn,
  ReturnType<typeof calculateMetrics>
>()

export function getResponseCharacterCount(parts: AdaptedContentPart[]): number {
  const cached = characterCounts.get(parts)
  if (cached !== undefined) return cached
  const { responseParts } = splitAssistantTurnParts(parts, false)
  const text = responseParts.map((part) => part.text).join("")
  // 保持 Unicode 码点口径；普通中英文直接使用长度，合并代理对。
  const pairs = text.match(/[\uD800-\uDBFF][\uDC00-\uDFFF]/g)
  const count = text.length - (pairs?.length ?? 0)
  characterCounts.set(parts, count)
  return count
}

function calculateMetrics(turn: MessageTurn) {
  let complete = true
  const toolIds = new Set<string>()
  for (const block of turn.blocks) {
    // 图片块没有原生调用 ID，不能把一张图片直接当作一次调用。
    if (block.type === "image_generation") complete = false
    if (block.type !== "tool_use") continue
    if (block.tool_use_id) toolIds.add(block.tool_use_id)
    else complete = false
  }
  return { toolCallIds: complete ? [...toolIds] : null }
}

export function getMessageOutputMetrics(turn: MessageTurn) {
  const cached = turnMetrics.get(turn)
  if (cached) return cached
  const metrics = calculateMetrics(turn)
  turnMetrics.set(turn, metrics)
  return metrics
}

export function mergeToolCallIds(groups: { toolCallIds: string[] | null }[]) {
  if (groups.some((group) => group.toolCallIds === null)) return null
  return [...new Set(groups.flatMap((group) => group.toolCallIds ?? []))]
}
