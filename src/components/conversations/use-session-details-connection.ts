import { useEffect, useState } from "react"
import { acpGetSessionSnapshotByConversation } from "@/lib/api"
import { onTransportReconnect, subscribe } from "@/lib/platform"
import type {
  ConnectionStatus,
  DbConversationSummary,
  EventEnvelope,
} from "@/lib/types"

type RuntimeStatus = ConnectionStatus | "unavailable"
type StatusEvent = { seq: number; status: ConnectionStatus }

interface ConnectionObserver {
  id: number
  externalId: string | null
  disposed: boolean
  generation: number
  connectionId: string | null
  events: Map<string, StatusEvent>
  update: (status: RuntimeStatus) => void
}

export function useSessionDetailsConnection(
  summary: DbConversationSummary,
  enabled: boolean
): RuntimeStatus | null {
  const [result, setResult] = useState<{
    id: number
    status: RuntimeStatus
  } | null>(null)
  const { id, external_id: externalId, status } = summary
  useEffect(() => {
    if (!enabled || id <= 0 || status !== "in_progress") return
    return observeConnection({ id, externalId }, (next) =>
      setResult({ id, status: next })
    )
  }, [enabled, externalId, id, status])
  return enabled && status === "in_progress" && result?.id === id
    ? result.status
    : null
}

function observeConnection(
  identity: { id: number; externalId: string | null },
  update: (status: RuntimeStatus) => void
): () => void {
  const observer: ConnectionObserver = {
    ...identity,
    disposed: false,
    generation: 0,
    connectionId: null,
    events: new Map(),
    update,
  }
  let unsubscribe = () => {}
  const reconnect = onTransportReconnect(() => void refreshConnection(observer))
  void subscribe<EventEnvelope>("acp://event", (event) =>
    applyConnectionEvent(observer, event)
  )
    .then((stop) => {
      if (observer.disposed) return stop()
      unsubscribe = stop
      void refreshConnection(observer)
    })
    .catch((error) => {
      if (observer.disposed) return
      console.error("[session-details] Connection subscription failed:", error)
      update("unavailable")
    })
  return () => {
    observer.disposed = true
    reconnect?.()
    unsubscribe()
  }
}

async function refreshConnection(observer: ConnectionObserver): Promise<void> {
  const current = ++observer.generation
  try {
    const snapshot = await acpGetSessionSnapshotByConversation(observer.id)
    if (observer.disposed || current !== observer.generation) return
    observer.connectionId = snapshot?.connection_id ?? null
    if (!snapshot) return observer.update("disconnected")
    const event = observer.events.get(snapshot.connection_id)
    const latest =
      event && event.seq > snapshot.event_seq
        ? event
        : { seq: snapshot.event_seq, status: snapshot.status }
    observer.events.set(snapshot.connection_id, latest)
    observer.update(latest.status)
  } catch (error) {
    if (observer.disposed || current !== observer.generation) return
    console.error("[session-details] Connection lookup failed:", error)
    observer.update("unavailable")
  }
}

function applyConnectionEvent(
  observer: ConnectionObserver,
  event: EventEnvelope
): void {
  if (observer.disposed) return
  if (event.type === "status_changed") {
    const previous = observer.events.get(event.connection_id)
    if (previous && previous.seq >= event.seq) return
    observer.events.set(event.connection_id, {
      seq: event.seq,
      status: event.status,
    })
    if (event.connection_id === observer.connectionId)
      observer.update(event.status)
  } else if (
    (event.type === "conversation_linked" &&
      event.conversation_id === observer.id) ||
    (event.type === "session_started" &&
      observer.externalId != null &&
      event.session_id === observer.externalId)
  ) {
    void refreshConnection(observer)
  }
}
