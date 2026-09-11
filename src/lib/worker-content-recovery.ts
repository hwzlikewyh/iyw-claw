import type { LiveContentBlock as WireContent } from "@/lib/types"
import type { LiveContentBlock } from "@/contexts/acp-connections-context"
import type { PlanEntryInfo } from "@/lib/types"

export function recoverWorkerContent(
  content: WireContent[],
  previous: LiveContentBlock[]
): LiveContentBlock[] {
  const tools = new Map(
    previous
      .filter((block) => block.type === "tool_call")
      .map((block) => [block.info.tool_call_id, block.info])
  )
  return content.flatMap((block): LiveContentBlock[] => {
    switch (block.kind) {
      case "text":
        return [{ type: "text", text: block.text }]
      case "thinking":
        return [{ type: "thinking", text: block.text }]
      case "tool_call_ref": {
        const info = tools.get(block.tool_call_id)
        return info ? [{ type: "tool_call", info }] : []
      }
      case "plan":
        return [{ type: "plan", entries: block.entries as PlanEntryInfo[] }]
      case "user_input":
        return [
          {
            type: "user_input",
            messageId: block.message_id,
            blocks: block.blocks,
            createdAt: Date.parse(block.created_at),
          },
        ]
    }
  })
}
