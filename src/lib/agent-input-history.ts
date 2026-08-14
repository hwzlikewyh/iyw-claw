import {
  parseFeedbackCheckOutcome,
  type FeedbackEntry,
} from "@/lib/feedback-check"
import { normalizeToolName } from "@/lib/tool-call-normalization"
import {
  appendPendingAgentInputs,
  buildAgentInputUserTurn,
} from "@/lib/agent-input-history-pending"
import type {
  AgentInputItem,
  ContentBlock,
  DbConversationDetail,
  MessageTurn,
} from "@/lib/types"

const TIMESTAMP_TOLERANCE_MS = 1_000

type ToolResultBlock = Extract<ContentBlock, { type: "tool_result" }>

interface BlockLocation {
  turnIndex: number
  blockIndex: number
}
interface FeedbackCall {
  toolUse: BlockLocation
  result: BlockLocation
  block: ToolResultBlock
}
interface FeedbackCandidate {
  item: AgentInputItem
  feedbackText: string
  createdAt: number
  consumedAt: number
  matched: boolean
}
interface HistoryPatches {
  removed: Set<string>
  rewritten: Map<string, ToolResultBlock>
  insertAfter: Map<string, AgentInputItem[]>
}
function locationKey(location: BlockLocation): string {
  return `${location.turnIndex}:${location.blockIndex}`
}

function feedbackText(item: AgentInputItem): string | null {
  const parts: string[] = []
  for (const block of item.payload.blocks) {
    if (block.type !== "text") return null
    const text = block.text.trim()
    if (text) parts.push(text)
  }
  const text = parts.join("\n")
  return text || null
}
function buildCandidates(
  inputs: AgentInputItem[],
  existingTurnIds: Set<string>
): FeedbackCandidate[] {
  return inputs
    .filter(
      (item) =>
        item.status === "consumed" &&
        item.strategy === "cooperative_feedback" &&
        !existingTurnIds.has(item.id)
    )
    .flatMap((item) => {
      const text = feedbackText(item)
      if (!text) return []
      return [
        {
          item,
          feedbackText: text,
          createdAt: Date.parse(item.created_at),
          consumedAt: Date.parse(item.consumed_at ?? item.created_at),
          matched: false,
        },
      ]
    })
    .sort((a, b) => a.createdAt - b.createdAt)
}
function isBefore(a: BlockLocation, b: BlockLocation): boolean {
  return (
    a.turnIndex < b.turnIndex ||
    (a.turnIndex === b.turnIndex && a.blockIndex < b.blockIndex)
  )
}
function takeIdlessToolUse(
  uses: BlockLocation[],
  result: BlockLocation
): BlockLocation | null {
  for (let index = uses.length - 1; index >= 0; index -= 1) {
    if (!isBefore(uses[index], result)) continue
    return uses.splice(index, 1)[0]
  }
  return null
}

function findFeedbackCalls(turns: MessageTurn[]): FeedbackCall[] {
  const usesById = new Map<string, BlockLocation>()
  const idlessUses: BlockLocation[] = []
  const calls: FeedbackCall[] = []

  turns.forEach((turn, turnIndex) => {
    if (turn.role !== "assistant") return
    turn.blocks.forEach((block, blockIndex) => {
      if (
        block.type !== "tool_use" ||
        normalizeToolName(block.tool_name) !== "check_user_feedback"
      ) {
        return
      }
      const location = { turnIndex, blockIndex }
      if (block.tool_use_id) usesById.set(block.tool_use_id, location)
      else idlessUses.push(location)
    })
  })

  turns.forEach((turn, turnIndex) => {
    if (turn.role !== "assistant") return
    turn.blocks.forEach((block, blockIndex) => {
      if (block.type !== "tool_result" || block.is_error) return
      const result = { turnIndex, blockIndex }
      const toolUse = block.tool_use_id
        ? usesById.get(block.tool_use_id)
        : takeIdlessToolUse(idlessUses, result)
      if (toolUse) calls.push({ toolUse, result, block })
    })
  })
  return calls
}

function matchesTimestamp(
  entry: FeedbackEntry,
  candidate: FeedbackCandidate
): boolean {
  if (!entry.createdAt) return true
  const entryAt = Date.parse(entry.createdAt)
  if (Number.isNaN(entryAt)) return true
  return (
    entryAt >= candidate.createdAt - TIMESTAMP_TOLERANCE_MS &&
    entryAt <= candidate.consumedAt + TIMESTAMP_TOLERANCE_MS
  )
}

function matchEntry(
  entry: FeedbackEntry,
  candidates: FeedbackCandidate[]
): FeedbackCandidate | null {
  const text = entry.text.trim()
  const candidate = candidates.find(
    (item) =>
      !item.matched &&
      item.feedbackText === text &&
      matchesTimestamp(entry, item)
  )
  if (!candidate) return null
  candidate.matched = true
  return candidate
}

function serializeFeedbackEntries(entries: FeedbackEntry[]): string {
  return JSON.stringify({
    count: entries.length,
    feedback: entries.map((entry) => ({
      created_at: entry.createdAt,
      text: entry.text,
    })),
  })
}

function buildHistoryPatches(
  calls: FeedbackCall[],
  candidates: FeedbackCandidate[]
): HistoryPatches {
  const patches: HistoryPatches = {
    removed: new Set(),
    rewritten: new Map(),
    insertAfter: new Map(),
  }
  for (const call of calls) {
    const outcome = parseFeedbackCheckOutcome(call.block.output_preview)
    if (!outcome?.entries.length) continue
    const matched: AgentInputItem[] = []
    const unmatched: FeedbackEntry[] = []
    for (const entry of outcome.entries) {
      const candidate = matchEntry(entry, candidates)
      if (candidate) matched.push(candidate.item)
      else unmatched.push(entry)
    }
    if (matched.length === 0) continue
    const resultKey = locationKey(call.result)
    patches.insertAfter.set(resultKey, matched)
    if (unmatched.length === 0) {
      patches.removed.add(locationKey(call.toolUse))
      patches.removed.add(resultKey)
    } else {
      patches.rewritten.set(resultKey, {
        ...call.block,
        output_preview: serializeFeedbackEntries(unmatched),
      })
    }
  }
  return patches
}

function buildAssistantSegment(
  turn: MessageTurn,
  blocks: ContentBlock[],
  index: number,
  timestamp: string
): MessageTurn {
  const { usage, duration_ms, completed_at, ...base } = turn
  return {
    ...base,
    id: index === 0 ? turn.id : `${turn.id}-agent-input-${index}`,
    blocks,
    timestamp,
  }
}

function restoreAssistantMetrics(
  segments: MessageTurn[],
  source: MessageTurn
): MessageTurn[] {
  for (let index = segments.length - 1; index >= 0; index -= 1) {
    if (segments[index].role !== "assistant") continue
    segments[index] = {
      ...segments[index],
      usage: source.usage,
      duration_ms: source.duration_ms,
      completed_at: source.completed_at,
    }
    break
  }
  return segments
}

function applyTurnPatches(
  turn: MessageTurn,
  turnIndex: number,
  patches: HistoryPatches
): MessageTurn[] {
  if (turn.role !== "assistant") return [turn]
  const output: MessageTurn[] = []
  let blocks: ContentBlock[] = []
  let segmentIndex = 0
  let timestamp = turn.timestamp
  const flush = () => {
    if (blocks.length === 0) return
    output.push(buildAssistantSegment(turn, blocks, segmentIndex, timestamp))
    blocks = []
    segmentIndex += 1
  }

  turn.blocks.forEach((block, blockIndex) => {
    const key = locationKey({ turnIndex, blockIndex })
    if (!patches.removed.has(key)) {
      blocks.push(patches.rewritten.get(key) ?? block)
    }
    const inserted = patches.insertAfter.get(key)
    if (!inserted) return
    flush()
    for (const item of inserted) output.push(buildAgentInputUserTurn(item))
    const lastInserted = inserted[inserted.length - 1]
    timestamp =
      lastInserted?.consumed_at ?? lastInserted?.created_at ?? timestamp
  })
  flush()
  return restoreAssistantMetrics(output, turn)
}

export function mergeAgentInputHistory(
  detail: DbConversationDetail,
  inputs: AgentInputItem[]
): DbConversationDetail {
  const existingTurnIds = new Set(detail.turns.map((turn) => turn.id))
  const candidates = buildCandidates(inputs, existingTurnIds)
  const patches = buildHistoryPatches(
    findFeedbackCalls(detail.turns),
    candidates
  )
  const patchedTurns =
    patches.insertAfter.size === 0
      ? detail.turns
      : detail.turns.flatMap((turn, index) =>
          applyTurnPatches(turn, index, patches)
        )
  const turns = appendPendingAgentInputs(patchedTurns, inputs)
  if (turns === detail.turns) return detail
  return {
    ...detail,
    turns,
  }
}
