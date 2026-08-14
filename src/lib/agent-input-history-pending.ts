import type { AgentInputItem, ContentBlock, MessageTurn } from "@/lib/types"

const DEFERRED_TRANSCRIPT_GRACE_MS = 5 * 60 * 1_000

function inputText(item: AgentInputItem): string {
  const displayText = item.payload.display_text.trim()
  if (displayText) return displayText
  const promptText = item.payload.blocks
    .flatMap((block) => (block.type === "text" ? [block.text.trim()] : []))
    .filter(Boolean)
    .join("\n")
  if (promptText) return promptText
  return item.payload.blocks
    .flatMap((block) => {
      if (block.type === "resource_link") return [block.name || block.uri]
      return block.type === "resource" ? [block.uri] : []
    })
    .map((value) => value.trim())
    .filter(Boolean)
    .join("\n")
}

export function buildAgentInputUserTurn(item: AgentInputItem): MessageTurn {
  const blocks: ContentBlock[] = item.payload.blocks.flatMap((block) => {
    if (block.type !== "image") return []
    return [
      {
        type: "image" as const,
        data: block.data,
        mime_type: block.mime_type,
        uri: block.uri ?? null,
      },
    ]
  })
  const text = inputText(item)
  if (text) blocks.push({ type: "text", text })
  return {
    id: item.id,
    role: "user",
    blocks,
    timestamp: item.consumed_at ?? item.created_at,
  }
}

function contentFingerprint(blocks: ContentBlock[]): string | null {
  const text = blocks
    .flatMap((block) => (block.type === "text" ? [block.text.trim()] : []))
    .filter(Boolean)
    .join("\n")
  const images = blocks.flatMap((block) => {
    if (block.type !== "image") return []
    const tail = block.data.slice(-64)
    return [
      `${block.mime_type}:${block.data.length}:${block.data.slice(0, 64)}:${tail}`,
    ]
  })
  return text || images.length > 0 ? JSON.stringify({ text, images }) : null
}

function inputFingerprints(item: AgentInputItem): Set<string> {
  const fingerprints = new Set<string>()
  const visible = contentFingerprint(buildAgentInputUserTurn(item).blocks)
  if (visible) fingerprints.add(visible)
  const promptBlocks: ContentBlock[] = item.payload.blocks.flatMap((block) => {
    if (block.type === "text")
      return [{ type: "text" as const, text: block.text }]
    if (block.type !== "image") return []
    return [{ ...block, uri: block.uri ?? null }]
  })
  const prompt = contentFingerprint(promptBlocks)
  if (prompt) fingerprints.add(prompt)
  return fingerprints
}

function isRecentDeferred(item: AgentInputItem): boolean {
  if (item.status !== "consumed" || item.strategy !== "deferred_next") {
    return false
  }
  const consumedAt = Date.parse(item.consumed_at ?? item.created_at)
  return (
    Number.isFinite(consumedAt) &&
    Date.now() - consumedAt <= DEFERRED_TRANSCRIPT_GRACE_MS
  )
}

function hasPersistedTurn(
  turns: MessageTurn[],
  item: AgentInputItem,
  claimedTurnIds: Set<string>
): boolean {
  const fingerprints = inputFingerprints(item)
  const sentAt = Date.parse(item.consumed_at ?? item.created_at)
  const match = turns.find((turn) => {
    if (turn.role !== "user" || claimedTurnIds.has(turn.id)) return false
    if (turn.id === item.id) return true
    const fingerprint = contentFingerprint(turn.blocks)
    if (!fingerprint || !fingerprints.has(fingerprint)) return false
    const turnAt = Date.parse(turn.timestamp)
    return (
      !Number.isFinite(sentAt) ||
      !Number.isFinite(turnAt) ||
      Math.abs(turnAt - sentAt) <= DEFERRED_TRANSCRIPT_GRACE_MS
    )
  })
  if (!match) return false
  claimedTurnIds.add(match.id)
  return true
}

export function appendPendingAgentInputs(
  turns: MessageTurn[],
  inputs: AgentInputItem[]
): MessageTurn[] {
  const existingIds = new Set(turns.map((turn) => turn.id))
  const claimedTurnIds = new Set<string>()
  const pending = inputs
    .filter((item) => {
      if (item.status === "consumed" || item.status === "deleted") {
        return (
          isRecentDeferred(item) &&
          !existingIds.has(item.id) &&
          !hasPersistedTurn(turns, item, claimedTurnIds)
        )
      }
      return !existingIds.has(item.id)
    })
    .sort(
      (a, b) =>
        a.sort_index - b.sort_index ||
        a.created_at.localeCompare(b.created_at) ||
        a.id.localeCompare(b.id)
    )
    .map(buildAgentInputUserTurn)
  return pending.length === 0 ? turns : [...turns, ...pending]
}
