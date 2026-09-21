"use client"

import { useCallback, useEffect, useRef } from "react"
import { scheduleAcpPreparation } from "@/lib/acp-session-preparation"
import type { AgentType } from "@/lib/types"

export function useSessionPreparation(target: {
  agentType: AgentType
  workingDir?: string
  conversationId?: number
  disabled?: boolean
}) {
  const { agentType, workingDir, conversationId, disabled } = target
  const cancelRef = useRef<(() => void) | null>(null)
  const cancel = useCallback(() => {
    cancelRef.current?.()
    cancelRef.current = null
  }, [])
  const prepare = useCallback(() => {
    cancel()
    if (!disabled) {
      cancelRef.current = scheduleAcpPreparation({
        agentType,
        workingDir,
        conversationId,
      })
    }
  }, [agentType, workingDir, conversationId, disabled, cancel])
  useEffect(
    () => cancel,
    [cancel, agentType, workingDir, conversationId, disabled]
  )
  return {
    onPointerEnter: prepare,
    onPointerLeave: cancel,
    onFocus: prepare,
    onBlur: cancel,
  }
}
