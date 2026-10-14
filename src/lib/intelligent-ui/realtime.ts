import { getTransport, onTransportChange } from "@/lib/transport"
import { UI_STATE_EVENT, type UiState } from "./api"
import {
  acceptRemoteState,
  flushUiStates,
  invalidateUiStates,
} from "./state-store"

const conversations = new Map<number, number>()
let dispose: (() => void) | undefined
let generation = 0
function connect() {
  dispose?.()
  const current = ++generation,
    transport = getTransport()
  const reconnect = transport.onReconnect?.(invalidateUiStates)
  dispose = () => {
    generation++
    reconnect?.()
  }
  void transport
    .subscribe<UiState>(UI_STATE_EVENT, (row) => {
      if (current === generation) acceptRemoteState(row)
    })
    .then((unsub) => {
      if (current !== generation) {
        unsub()
        return
      }
      dispose = () => {
        generation++
        unsub()
        reconnect?.()
      }
    })
    .catch(() => {})
}
function flush() {
  for (const id of conversations.keys()) void flushUiStates(id).catch(() => {})
}
export function retainUiStateEvents(conversationId: number) {
  if (!conversations.size) {
    connect()
    window.addEventListener("pagehide", flush)
  }
  conversations.set(
    conversationId,
    (conversations.get(conversationId) ?? 0) + 1
  )
  return () => {
    const remaining = (conversations.get(conversationId) ?? 1) - 1
    if (remaining > 0) conversations.set(conversationId, remaining)
    else {
      conversations.delete(conversationId)
      void flushUiStates(conversationId).catch(() => {})
    }
    if (!conversations.size) {
      dispose?.()
      dispose = undefined
      window.removeEventListener("pagehide", flush)
    }
  }
}
onTransportChange(() => {
  if (conversations.size) connect()
})
