import { getAgentModeState as getDefinedAgentModeState } from "@/lib/agent-option-definitions"
import type {
  AgentType,
  SessionModeInfo,
  SessionModeStateInfo,
} from "@/lib/types"

export function getAgentModeState(agentType: AgentType): SessionModeStateInfo {
  const state = getDefinedAgentModeState(agentType)
  return {
    ...state,
    available_modes: state.available_modes.map((mode) => ({ ...mode })),
  }
}

export function getAgentModes(agentType?: AgentType): SessionModeInfo[] {
  return agentType ? getAgentModeState(agentType).available_modes : []
}
