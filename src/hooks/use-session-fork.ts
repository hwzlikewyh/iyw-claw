import { useCallback, useRef, useState } from "react"
import { acpFork, type ForkResult } from "@/lib/api"
import { toErrorMessage } from "@/lib/app-error"

interface SessionForkOptions {
  connectionId: string | null
  conversationId: number | null
  folderId: number
  canFork: boolean
  getQueueLength: () => number
  onForked: (result: ForkResult) => void
}

export function useSessionFork(options: SessionForkOptions) {
  const pendingRef = useRef(false)
  const [pending, setPending] = useState(false)
  const fork = useCallback(async () => {
    const { connectionId, conversationId, folderId } = options
    if (
      !options.canFork ||
      !connectionId ||
      !conversationId ||
      pendingRef.current ||
      options.getQueueLength() > 0
    )
      return false
    pendingRef.current = true
    setPending(true)
    console.info("[session-fork] started", { conversationId })
    try {
      const result = await acpFork(connectionId, conversationId, folderId)
      options.onForked(result)
      console.info("[session-fork] completed", {
        conversationId,
        siblingConversationId: result.siblingConversationId,
      })
      return true
    } catch (error) {
      console.error("[session-fork] failed", {
        conversationId,
        error: toErrorMessage(error),
      })
      throw error
    } finally {
      pendingRef.current = false
      setPending(false)
    }
  }, [options])
  return { fork, pending, pendingRef }
}
