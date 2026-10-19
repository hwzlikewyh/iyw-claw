import type {
  LiveMessage,
  ToolCallInfo,
} from "@/contexts/acp-connections-context"
import type { PlanEntryInfo } from "@/lib/types"

const EMPTY_PLAN: PlanEntryInfo[] = []

interface TurnSummary {
  activeTool: ToolCallInfo | null
  planEntries: PlanEntryInfo[]
  lastContent: "text" | "thinking" | null
  latestKind?: "text" | "thinking" | "tool_call" | "plan" | null
  latestTool?: ToolCallInfo | null
}

export function summarizeLiveTurn(message: LiveMessage | null): TurnSummary {
  let activeTool: ToolCallInfo | null = null
  let pendingTool: ToolCallInfo | null = null
  let planEntries = EMPTY_PLAN
  let lastContent: "text" | "thinking" | null = null
  let latestKind: TurnSummary["latestKind"] = null
  let latestTool: ToolCallInfo | null = null
  const blocks = message?.content ?? []
  for (let index = blocks.length - 1; index >= 0; index -= 1) {
    const block = blocks[index]
    if (!latestKind && block.type !== "user_input") latestKind = block.type
    if (block.type === "plan" && planEntries === EMPTY_PLAN)
      planEntries = block.entries
    if (!lastContent && (block.type === "text" || block.type === "thinking"))
      lastContent = block.type
    if (block.type !== "tool_call") continue
    latestTool ??= block.info
    if (
      !activeTool &&
      ["in_progress", "inprogress"].includes(block.info.status)
    )
      activeTool = block.info
    if (!pendingTool && block.info.status === "pending")
      pendingTool = block.info
  }
  return {
    activeTool: activeTool ?? pendingTool,
    planEntries,
    lastContent,
    latestKind,
    latestTool,
  }
}

export function readableStatusText(text: string): string | null {
  const value = text.trim()
  if (
    !value ||
    /[/\\`\n{}]|\b(?:mcp__|functions\.|iyw-claw)|\w+\.\w+/i.test(value)
  )
    return null
  return value
}
