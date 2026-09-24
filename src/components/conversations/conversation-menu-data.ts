import { requestConversationHistory } from "@/lib/conversation-history-request"
import { mergeAgentInputHistory } from "@/lib/agent-input-history"
import { getTransport } from "@/lib/transport"
import type {
  AgentInputItem,
  ContentBlock,
  DbConversationDetail,
  MessageTurn,
} from "@/lib/types"

export async function loadConversationMenuHistory(
  conversationId: number,
  signal: AbortSignal
): Promise<DbConversationDetail> {
  const transport = getTransport()
  const load = async (before?: number) => {
    signal.throwIfAborted()
    const request = { conversationId, before, forceRefresh: false }
    let page = await requestConversationHistory(transport, request)
    signal.throwIfAborted()
    if (page.history_stale) {
      page = await requestConversationHistory(transport, {
        ...request,
        forceRefresh: true,
      })
    }
    signal.throwIfAborted()
    return page
  }
  const latest = await load()
  const pages: MessageTurn[][] = [latest.turns]
  let before = latest.history_start
  while (before > 0) {
    const page = await load(before)
    if (page.history_start >= before || page.turns.length === 0) {
      throw new Error("Conversation history pagination did not advance")
    }
    if (
      page.history_total_turns !== latest.history_total_turns ||
      page.transcript_watermark !== latest.transcript_watermark
    ) {
      throw new Error("Conversation history changed during loading; retry")
    }
    pages.push(page.turns)
    before = page.history_start
  }
  const inputs = await transport.call<AgentInputItem[]>("list_agent_inputs", {
    conversationId,
  })
  signal.throwIfAborted()
  return mergeAgentInputHistory(
    { ...latest, turns: pages.reverse().flat(), history_start: 0 },
    inputs
  )
}

function searchableBlock(block: ContentBlock): string {
  switch (block.type) {
    case "text":
    case "thinking":
      return block.text
    case "tool_use":
      return [block.tool_name, block.input_preview].filter(Boolean).join("\n")
    case "tool_result":
      return block.output_preview ?? ""
    case "display_image":
      return block.caption ?? block.name
    case "image_generation":
      return block.revised_prompt ?? ""
    default:
      return ""
  }
}

export function conversationTurnText(turn: MessageTurn): string {
  return turn.blocks.map(searchableBlock).filter(Boolean).join("\n\n")
}
