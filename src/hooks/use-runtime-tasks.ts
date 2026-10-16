"use client"

import { useCallback, useEffect, useReducer, useRef } from "react"
import type { Dispatch, RefObject } from "react"
import { useAcpEvent } from "@/contexts/acp-connections-context"
import {
  controlRuntimeTask,
  loadRuntimeTasks,
  type RuntimeTaskSnapshot,
  type RuntimeTask,
} from "@/lib/runtime-tasks"
import { toErrorMessage } from "@/lib/app-error"
import type { EventEnvelope } from "@/lib/types"
import { getTransport } from "@/lib/transport"

const EMPTY: RuntimeTaskSnapshot = { tasks: [], revision: 0 }
const REFRESH_DELAY_MS = 150
type State = {
  scope: string | null
  snapshot: RuntimeTaskSnapshot
  error: string | null
  pending: string | null
}
const INITIAL: State = {
  scope: null,
  snapshot: EMPTY,
  error: null,
  pending: null,
}
const merge = (state: State, patch: Partial<State>): State => ({
  ...state,
  ...patch,
})
interface Context {
  id: string | null
  connectionId: string | null
  conversationId: number | null
  currentRef: RefObject<string | null>
  versionRef: RefObject<number>
  controlPendingRef: RefObject<symbol | null>
  dispatch: Dispatch<Partial<State>>
}

export function useRuntimeTasks(
  connectionId: string | null,
  conversationId: number | null
) {
  const id =
    connectionId || conversationId != null
      ? JSON.stringify([connectionId, conversationId])
      : null
  const [state, dispatch] = useReducer(merge, INITIAL)
  const currentRef = useRef(id)
  const versionRef = useRef(0)
  const controlPendingRef = useRef<symbol | null>(null)
  const context: Context = {
    id,
    connectionId,
    conversationId,
    currentRef,
    versionRef,
    controlPendingRef,
    dispatch,
  }
  const refresh = useTaskRefresh(context)
  const control = useTaskControl(context, refresh)
  useTaskEvents(
    connectionId,
    refresh,
    state.scope === id ? state.snapshot.tasks : EMPTY.tasks
  )
  useTaskLifecycle(context, refresh)
  const visible = state.scope === id ? state : INITIAL
  return {
    ...visible.snapshot,
    error: visible.error,
    pending: visible.pending,
    refresh,
    control,
  }
}

function useTaskEvents(
  connectionId: string | null,
  refresh: () => Promise<void>,
  tasks: RuntimeTask[]
) {
  const timer = useRef<ReturnType<typeof setTimeout> | null>(null)
  useEffect(
    () => () => {
      if (timer.current) clearTimeout(timer.current)
      timer.current = null
    },
    [connectionId]
  )
  useAcpEvent(
    useCallback(
      (event: EventEnvelope) => {
        if (
          event.connection_id !== connectionId &&
          !tasks.some(
            (task) => task.child_connection_id === event.connection_id
          )
        )
          return
        if (
          ![
            "runtime_tasks_changed",
            "session_started",
            "status_changed",
            "delegation_started",
            "delegation_completed",
            "permission_request",
            "permission_resolved",
            "question_request",
            "question_resolved",
          ].includes(event.type) ||
          timer.current
        )
          return
        timer.current = setTimeout(() => {
          timer.current = null
          void refresh()
        }, REFRESH_DELAY_MS)
      },
      [connectionId, refresh, tasks]
    )
  )
}

function useTaskRefresh({
  id,
  connectionId,
  conversationId,
  currentRef,
  versionRef,
  dispatch,
}: Context) {
  return useCallback(async () => {
    if (!id) return
    const request = ++versionRef.current
    try {
      const snapshot = await loadRuntimeTasks(connectionId, conversationId)
      if (currentRef.current === id && request === versionRef.current)
        dispatch({ snapshot, error: null })
    } catch (cause) {
      if (currentRef.current === id && request === versionRef.current)
        dispatch({ error: toErrorMessage(cause) })
    }
  }, [id, connectionId, conversationId, currentRef, versionRef, dispatch])
}

function useTaskControl(
  { id, connectionId, currentRef, controlPendingRef, dispatch }: Context,
  refresh: () => Promise<void>
) {
  return useCallback(
    async (taskId: string, action: string, nativeId: string) => {
      if (!id || !connectionId || controlPendingRef.current) return
      const token = Symbol()
      controlPendingRef.current = token
      dispatch({ pending: taskId, error: null })
      try {
        await controlRuntimeTask(connectionId, {
          task_id: taskId,
          native_id: nativeId,
          action,
        })
        if (currentRef.current === id) await refresh()
      } catch (cause) {
        if (currentRef.current === id)
          dispatch({ error: toErrorMessage(cause) })
      } finally {
        if (currentRef.current === id && controlPendingRef.current === token) {
          controlPendingRef.current = null
          dispatch({ pending: null })
        }
      }
    },
    [id, connectionId, currentRef, controlPendingRef, dispatch, refresh]
  )
}

function useTaskLifecycle(
  { id, currentRef, controlPendingRef, dispatch }: Context,
  refresh: () => Promise<void>
) {
  useEffect(() => {
    currentRef.current = id
    controlPendingRef.current = null
    dispatch({ ...INITIAL, scope: id })
    void refresh()
    const onVisible = () => {
      if (document.visibilityState === "visible") void refresh()
    }
    document.addEventListener("visibilitychange", onVisible)
    window.addEventListener("online", onVisible)
    const reconnect = getTransport().onReconnect?.(() => void refresh())
    return () => {
      currentRef.current = null
      document.removeEventListener("visibilitychange", onVisible)
      window.removeEventListener("online", onVisible)
      reconnect?.()
    }
  }, [id, refresh, currentRef, controlPendingRef, dispatch])
}
