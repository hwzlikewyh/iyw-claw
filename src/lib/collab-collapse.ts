/**
 * Live collab-capsule collapse (codex sub-agents).
 *
 * codex emits each collab op (spawn / wait / close) as its own ACP `tool_call`,
 * so a single sub-agent interaction streams as several capsules. This pure pass,
 * run at the top of `buildStreamingTurnsFromLiveMessage`, consolidates them to
 * match the history (Rust) reconstruction and the user's mental model:
 *
 *   - spawn  → the EXECUTION capsule. Its per-agent status is aggregated across
 *     all ops so it no longer freezes at the spawn-time "pendingInit"; the full
 *     result text is NOT shown here (it lives in the wait capsule).
 *   - wait   → preserves returned results; global waits show known child tasks.
 *   - close  → dropped (folded into the execution capsule's terminal status),
 *     unless it targets an agent with no spawn in this message (orphan — kept so
 *     nothing is lost).
 *
 * Non-collab blocks (and every block when there are no collab calls) pass
 * through untouched, so non-codex agents are entirely unaffected.
 */

import type { LiveContentBlock } from "@/contexts/acp-connections-context"
import { projectCollabActivity } from "./collab-activity"
import {
  isCodexCollabInput,
  parseCollabToolInput,
  classifyCollabOp,
  classifyCollabStatus,
  isErrorCollabStatusKind,
  mergeCollabAgentStatus,
} from "./collab-tool"

type CollabAgg = {
  /** Raw status strings across every op that referenced this agent, in order. */
  statuses: (string | null)[]
  /** Latest non-empty progress/result message seen for this agent. */
  lastMessage: string | null
  /** Whether some `wait` reported this agent (→ result shown in a wait capsule). */
  hasWait: boolean
  task: string | null
  name: string | null
}

type CollabToolCallBlock = Extract<LiveContentBlock, { type: "tool_call" }>

function isCollabBlock(block: LiveContentBlock): block is CollabToolCallBlock {
  return block.type === "tool_call" && isCodexCollabInput(block.info.raw_input)
}

/** Rewrite a spawn block into the execution capsule: aggregated per-agent status,
 *  result message dropped (unless the agent was never waited on), and an ACP
 *  status that reflects the agent lifecycle so the capsule shimmers while running
 *  and settles when done. */
function rewriteExecutionBlock(
  block: CollabToolCallBlock,
  agg: Map<string, CollabAgg>
): LiveContentBlock {
  const raw = block.info.raw_input
  if (!raw) return block
  const info = parseCollabToolInput(raw)
  if (!info) return block
  const parsed = JSON.parse(raw)
  const agents = info.agents

  const newStates: Record<string, unknown> = {}
  let anyError = false
  let anyActive = false
  let anyTerminal = false
  for (const value of agents) {
    const agentId = value.threadId
    const entry = agg.get(agentId)!
    const merged = mergeCollabAgentStatus(entry.statuses)
    const kind = classifyCollabStatus(merged)
    newStates[agentId] = {
      status: merged,
      message: entry.hasWait ? null : entry.lastMessage,
      task: entry.task,
      name: entry.name,
    }
    if (isErrorCollabStatusKind(kind)) anyError = true
    else if (
      kind === "completed" ||
      kind === "closed" ||
      kind === "interrupted"
    )
      anyTerminal = true
    else if (kind === "running" || kind === "pending") anyActive = true
  }

  const newRawInput = JSON.stringify({ ...parsed, agentsStates: newStates })
  const status = anyError
    ? "failed"
    : anyActive
      ? "in_progress"
      : anyTerminal
        ? "completed"
        : block.info.status

  return {
    type: "tool_call",
    info: { ...block.info, raw_input: newRawInput, status },
  }
}

function enrichWaitBlock(
  block: CollabToolCallBlock,
  context: { agg: Map<string, CollabAgg>; known: Set<string> }
): LiveContentBlock {
  const info = parseCollabToolInput(block.info.raw_input)
  if (!info) return block
  const agents = info.agents.length
    ? info.agents
    : [...context.known].map((threadId) => ({
        threadId,
        status: null,
        message: null,
      }))
  const states = Object.fromEntries(
    agents.map((agent) => {
      const entry = context.agg.get(agent.threadId)
      return [
        agent.threadId,
        {
          ...agent,
          status: agent.status ?? mergeCollabAgentStatus(entry?.statuses ?? []),
          message: agent.message,
          task: entry?.task,
          name: entry?.name,
        },
      ]
    })
  )
  return {
    ...block,
    info: {
      ...block.info,
      raw_input: JSON.stringify({
        ...JSON.parse(block.info.raw_input!),
        agentsStates: states,
      }),
    },
  }
}

function collectCollabContext(content: LiveContentBlock[]) {
  const agg = new Map<string, CollabAgg>()
  const spawnAgentIds = new Set<string>()
  for (const block of content) {
    if (!isCollabBlock(block)) continue
    const op = classifyCollabOp(block.info.title)
    const info = parseCollabToolInput(block.info.raw_input)
    if (!info) continue
    for (const a of info.agents) {
      const entry = agg.get(a.threadId) ?? {
        statuses: [],
        lastMessage: null,
        hasWait: false,
        task: null,
        name: null,
      }
      if (a.status === "running" || a.status === "inProgress") {
        entry.statuses = []
        entry.lastMessage = null
      }
      entry.statuses.push(a.status)
      if (a.message) entry.lastMessage = a.message
      if (op === "spawn" && info.prompt) entry.task = info.prompt
      if (a.name) entry.name = a.name
      if (op === "wait") entry.hasWait = true
      agg.set(a.threadId, entry)
      if (op === "spawn") spawnAgentIds.add(a.threadId)
    }
  }
  return { agg, spawnAgentIds }
}

export function collapseLiveCollabBlocks(
  content: LiveContentBlock[]
): LiveContentBlock[] {
  const projected = content.map(projectCollabActivity)
  if (!projected.some(isCollabBlock)) return content
  const { agg, spawnAgentIds } = collectCollabContext(projected)
  // Pass 2 — rebuild: drop close, rewrite spawn, keep wait + everything else.
  const result: LiveContentBlock[] = []
  const known = new Set<string>()
  for (const block of projected) {
    if (!isCollabBlock(block)) {
      result.push(block)
      continue
    }
    const op = classifyCollabOp(block.info.title)
    const agents = parseCollabToolInput(block.info.raw_input)?.agents ?? []
    for (const agent of agents) known.add(agent.threadId)
    if (op === "wait") {
      result.push(enrichWaitBlock(block, { agg, known }))
      continue
    }
    if (
      block.info.title === "subAgentActivity" &&
      agents.every((a) => spawnAgentIds.has(a.threadId))
    )
      continue
    if (op === "close") {
      const ids =
        parseCollabToolInput(block.info.raw_input)?.agents.map(
          (a) => a.threadId
        ) ?? []
      // Drop only when every targeted agent has a spawn capsule to absorb it.
      if (ids.length > 0 && ids.every((id) => spawnAgentIds.has(id))) continue
      result.push(block)
      continue
    }
    if (op === "spawn") {
      result.push(rewriteExecutionBlock(block, agg))
      continue
    }
    result.push(block)
  }
  return result
}
