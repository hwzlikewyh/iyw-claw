import { useEffect, type MutableRefObject } from "react"
import { toast } from "sonner"
import {
  useAcpActions,
  useConnectionStore,
  type AcpActionsValue,
  type ConnectionStoreApi,
} from "@/contexts/acp-connections-context"
import { getShellTransport, isDesktop } from "@/lib/transport"
import { isDetachedConversationWindow } from "@/lib/conversation-window-state"
import { restoreConversationWindow } from "@/lib/conversation-window-transfer"
import { toErrorMessage } from "@/lib/app-error"

const RECEIVE_INTERVAL_MS = 500
interface ReceiveOptions {
  ready: boolean
  busy: MutableRefObject<boolean>
  setMoving: (value: boolean) => void
}

async function receiveReturn(
  label: string,
  context: ReceiveOptions & {
    actions: AcpActionsValue
    connections: ConnectionStoreApi
  }
) {
  context.busy.current = true
  context.setMoving(true)
  try {
    await restoreConversationWindow(label, context.actions, {
      connections: context.connections,
      append: true,
    })
  } catch (error) {
    console.error("[conversation-window] return failed", error)
    toast.error(toErrorMessage(error))
    await getShellTransport()
      .call("finish_conversation_window", { label })
      .catch((cleanupError) =>
        console.warn(
          "[conversation-window] return cleanup failed",
          cleanupError
        )
      )
  } finally {
    context.busy.current = false
    context.setMoving(false)
  }
}

export function useReturnedConversation({
  ready,
  busy,
  setMoving,
}: ReceiveOptions) {
  const actions = useAcpActions()
  const connections = useConnectionStore()
  useEffect(() => {
    if (!ready || !isDesktop() || isDetachedConversationWindow()) return
    let active = true
    let polling = false
    let failed = false
    const receive = async () => {
      if (!active || polling || busy.current) return
      polling = true
      try {
        const label = await getShellTransport().call<string | null>(
          "pending_conversation_window"
        )
        failed = false
        if (label && active && !busy.current) {
          await receiveReturn(label, {
            ready,
            busy,
            setMoving,
            actions,
            connections,
          })
        }
      } catch (error) {
        if (!failed)
          console.warn("[conversation-window] return lookup failed", error)
        failed = true
      } finally {
        polling = false
      }
    }
    const timer = setInterval(() => void receive(), RECEIVE_INTERVAL_MS)
    return () => {
      active = false
      clearInterval(timer)
    }
  }, [actions, connections, ready, busy, setMoving])
}
