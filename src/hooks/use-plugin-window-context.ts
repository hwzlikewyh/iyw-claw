import { useEffect, useLayoutEffect, useRef } from "react"
import {
  notifyPluginWindowContext,
  registerPluginWindow,
} from "@/lib/plugin-window-context"
import { getAgentDisplayName } from "@/lib/agent-sdk-presentation"
import type {
  AgentType,
  PromptDraft,
  SessionConfigOptionInfo,
} from "@/lib/types"
import { isModelConfigOption } from "@/lib/model-config-groups"

type WindowInput = {
  tabId: string
  conversationId: number | null
  agentType: AgentType
  modelId?: string
  options: SessionConfigOptionInfo[]
  send: (draft: PromptDraft) => boolean | void | Promise<boolean | void>
}

function windowSnapshot(value: WindowInput) {
  if (value.conversationId == null) return null
  const option = value.options.find(isModelConfigOption)
  const id =
    value.modelId ||
    (option?.kind.type === "select" ? option.kind.current_value : "")
  const name =
    option?.kind.type === "select"
      ? option.kind.options.find((entry) => entry.value === id)?.name
      : ""
  return {
    conversationId: value.conversationId,
    agentLabel: getAgentDisplayName(value.agentType),
    modelId: id || "",
    modelName: name || "",
  }
}

export function usePluginWindowContext(input: WindowInput) {
  const latest = useRef(input)
  useLayoutEffect(() => {
    latest.current = input
  }, [input])
  const model = input.options.find(isModelConfigOption)
  const modelId =
    input.modelId ||
    (model?.kind.type === "select" ? model.kind.current_value : "")
  useEffect(
    () =>
      registerPluginWindow(input.tabId, {
        context: () => windowSnapshot(latest.current),
        send: (draft) => latest.current.send(draft),
      }),
    [input.tabId]
  )
  useEffect(notifyPluginWindowContext, [
    input.conversationId,
    input.agentType,
    modelId,
  ])
}
