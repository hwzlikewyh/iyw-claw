import { useCallback, useEffect, useRef } from "react"

import { onTransportReconnect, subscribe } from "@/lib/platform"
import { invalidateTaskArtifactRequests } from "@/lib/task-artifact-request"
import { getTransport } from "@/lib/transport"
import { useAppWorkspaceStore } from "@/stores/app-workspace-store"

const REFRESH_DEBOUNCE_MS = 80

interface ArtifactSelection {
  conversationId: number | null
  latestTurnOnly?: boolean
}

export function useTaskArtifactUpdates(
  refresh: () => void,
  { conversationId, latestTurnOnly }: ArtifactSelection
) {
  const timerRef = useRef<ReturnType<typeof setTimeout> | null>(null)
  const schedule = useCallback(() => {
    invalidateTaskArtifactRequests(getTransport())
    if (timerRef.current) clearTimeout(timerRef.current)
    timerRef.current = setTimeout(() => {
      timerRef.current = null
      refresh()
    }, REFRESH_DEBOUNCE_MS)
  }, [refresh])

  useEffect(() => {
    let disposed = false
    let unsubscribe: (() => void) | undefined
    void subscribe<{ conversationId?: number }>(
      "task-artifact://changed",
      (event) => {
        if (
          shouldRefresh(conversationId, event?.conversationId, latestTurnOnly)
        ) {
          schedule()
        }
      }
    ).then((stop) => {
      if (disposed) stop()
      else unsubscribe = stop
    })
    const stopReconnect = onTransportReconnect(schedule)
    return () => {
      disposed = true
      unsubscribe?.()
      stopReconnect?.()
      if (timerRef.current) clearTimeout(timerRef.current)
    }
  }, [conversationId, latestTurnOnly, schedule])
}

function shouldRefresh(
  selectedId: number | null,
  changedId: number | undefined,
  exact: boolean | undefined
): boolean {
  if (selectedId == null || changedId == null || selectedId === changedId) {
    return true
  }
  if (exact) return false
  const conversations = useAppWorkspaceStore.getState().conversations
  const selected = conversations.find((item) => item.id === selectedId)
  const changed = conversations.find((item) => item.id === changedId)
  // 仅排除已知的不同根会话；缺少子会话关系时仍刷新，保留成果继承。
  return !(selected?.parent_id === null && changed?.parent_id === null)
}
