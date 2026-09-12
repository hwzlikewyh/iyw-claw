import { useCallback, useRef, useState } from "react"
import { acpFork, type ForkResult, type ForkTarget } from "@/lib/api"
import { toErrorMessage } from "@/lib/app-error"

interface SessionForkOptions {
  connectionId: string | null
  conversationId: number | null
  folderId: number
  canFork: boolean
  getQueueLength: () => number
  onForked: (result: ForkResult) => void
}

async function requestFork(request: Parameters<typeof acpFork>[0]) {
  const { conversationId } = request
  console.info("[session-fork] started", {
    conversationId,
    historical: Boolean(request.target),
  })
  try {
    const result = await acpFork(request)
    console.info("[session-fork] completed", {
      conversationId,
      siblingConversationId: result.siblingConversationId,
    })
    return result
  } catch (error) {
    console.error("[session-fork] failed", {
      conversationId,
      error: toErrorMessage(error),
    })
    throw error
  }
}

export function useSessionFork(options: SessionForkOptions) {
  const pendingRef = useRef(false)
  const [pending, setPending] = useState(false)
  const [pendingMessageId, setPendingMessageId] = useState<string | null>(null)
  const fork = useCallback(
    async (target?: ForkTarget) => {
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
      setPendingMessageId(target?.messageId ?? null)
      setPending(true)
      try {
        const result = await requestFork({
          connectionId,
          conversationId,
          folderId,
          target,
        })
        options.onForked(result)
        return true
      } finally {
        pendingRef.current = false
        setPending(false)
        setPendingMessageId(null)
      }
    },
    [options]
  )
  return { fork, pending, pendingRef, pendingMessageId }
}
