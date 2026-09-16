import type { LiveContentBlock } from "@/contexts/acp-connections-context"

const ACTIVITY_STATUS: Record<string, string> = {
  started: "running",
  completed: "completed",
  interrupted: "interrupted",
}

/** 将原生子线程生命周期投影到现有协作卡片，不改变原始消息流。 */
export function projectCollabActivity(
  block: LiveContentBlock
): LiveContentBlock {
  if (block.type !== "tool_call" || !block.info.raw_input) return block
  let input: Record<string, unknown>
  try {
    input = JSON.parse(block.info.raw_input)
  } catch {
    return block
  }
  if (!input || typeof input !== "object") return block
  const { agentThreadId, agentPath, activityKind } = input
  if (typeof agentThreadId !== "string" || typeof activityKind !== "string")
    return block
  return {
    ...block,
    info: {
      ...block.info,
      title: "subAgentActivity",
      raw_input: JSON.stringify({
        senderThreadId: "",
        receiverThreadIds: [agentThreadId],
        agentsStates: {
          [agentThreadId]: {
            status: ACTIVITY_STATUS[activityKind] ?? null,
            name: typeof agentPath === "string" ? agentPath : null,
            message: null,
          },
        },
      }),
    },
  }
}
