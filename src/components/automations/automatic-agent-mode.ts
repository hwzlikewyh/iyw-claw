import { getAgentModeState } from "@/lib/agent-modes"
import type { AgentType, SessionModeInfo } from "@/lib/types"

const AUTOMATIC_MODE_PRIORITY = [
  "agent-full-access",
  "bypassPermissions",
  "yolo",
  "dontAsk",
  "auto",
  "build",
  "act",
  "acceptEdits",
  "agent",
  "default",
]

export function automaticAgentMode(
  agentType: AgentType
): SessionModeInfo | null {
  const modes = getAgentModeState(agentType).available_modes
  for (const id of AUTOMATIC_MODE_PRIORITY) {
    const mode = modes.find((candidate) => candidate.id === id)
    if (mode) return mode
  }
  return modes[0] ?? null
}
