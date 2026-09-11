"use client"

import { useCallback, useEffect, useRef, useState } from "react"
import { acpSideQuestion } from "@/lib/api"
import type { SideQuestionEntry, SideQuestionResult } from "@/lib/side-question"

export function useSideQuestion(
  connectionId: string | null,
  sessionId: string | null,
  enabled: boolean
) {
  const [open, setOpen] = useState(false)
  const [capability, setCapability] = useState<SideQuestionResult | null>(null)
  const [entries, setEntries] = useState<SideQuestionEntry[]>([])
  const [error, setError] = useState<string | null>(null)
  const generation = useRef(0)
  const active = useRef<{
    id: string
    connectionId: string
    sessionId: string
  } | null>(null)

  useEffect(() => {
    const epoch = ++generation.current
    setCapability(null)
    setEntries([])
    setError(null)
    setOpen(false)
    if (enabled && connectionId && sessionId) {
      void acpSideQuestion(connectionId, { sessionId, action: "capabilities" })
        .then((result) => {
          if (generation.current === epoch) setCapability(result)
        })
        .catch((cause) => {
          if (generation.current === epoch) setError(String(cause))
        })
    }
    return () => {
      ++generation.current
      const request = active.current
      active.current = null
      if (request) {
        void acpSideQuestion(request.connectionId, {
          sessionId: request.sessionId,
          action: "cancel",
          requestId: request.id,
        }).catch(() => {
          console.warn(
            "[side-question] unmount cancellation was not acknowledged"
          )
        })
      }
    }
  }, [connectionId, sessionId, enabled])

  const ask = useCallback(
    (question: string): boolean => {
      setOpen(true)
      if (!question.trim()) return true
      if (
        !enabled ||
        !capability?.supported ||
        !connectionId ||
        !sessionId ||
        active.current
      )
        return false
      const epoch = generation.current
      const id = crypto.randomUUID()
      active.current = { id, connectionId, sessionId }
      setError(null)
      const entry: SideQuestionEntry = { id, question, status: "running" }
      setEntries((old) => [...old, entry].slice(-20))
      void acpSideQuestion(connectionId, {
        sessionId,
        action: "ask",
        requestId: id,
        question,
      })
        .then((result) => {
          if (generation.current !== epoch) return
          if (active.current?.id === id) active.current = null
          const status = result.cancelled
            ? "cancelled"
            : result.synthetic
              ? "failed"
              : typeof result.response === "string"
                ? "completed"
                : "failed"
          setEntries((old) =>
            old.map((entry) =>
              entry.id === id
                ? {
                    ...entry,
                    status,
                    answer: result.synthetic ? undefined : result.response,
                    error: result.synthetic ? result.response : undefined,
                  }
                : entry
            )
          )
        })
        .catch((cause) => {
          if (generation.current !== epoch) return
          // A transport failure is not proof the native request stopped.
          void acpSideQuestion(connectionId, {
            sessionId,
            action: "cancel",
            requestId: id,
          }).catch(() =>
            console.warn(
              "[side-question] failure cancellation was not acknowledged"
            )
          )
          if (active.current?.id === id) active.current = null
          setEntries((old) =>
            old.map((entry) =>
              entry.id === id
                ? { ...entry, status: "failed", error: String(cause) }
                : entry
            )
          )
        })
        .finally(() => {
          if (active.current?.id === id) active.current = null
        })
      return true
    },
    [enabled, capability, connectionId, sessionId]
  )

  const cancel = useCallback(async () => {
    const request = active.current
    if (!request) return
    const epoch = generation.current
    try {
      await acpSideQuestion(request.connectionId, {
        sessionId: request.sessionId,
        action: "cancel",
        requestId: request.id,
      })
      // Keep the request active until the original ask settles and cleans up.
    } catch (cause) {
      if (generation.current === epoch) setError(String(cause))
    }
  }, [])

  return {
    open,
    setOpen,
    entries,
    capability,
    error,
    ask,
    cancel,
    running: entries.some((entry) => entry.status === "running"),
  }
}
