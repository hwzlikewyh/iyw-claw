import { useCallback, useRef, useState } from "react"
import { subscribe } from "@/lib/platform"
import type { AgentInstallEvent, AgentInstallEventKind } from "@/lib/types"

const AGENT_INSTALL_EVENT = "app://agent-install"

export type AgentInstallStatus = "idle" | "running" | "success" | "failed"

interface AgentInstallStreamState {
  status: AgentInstallStatus
  progress: number // 0-100, meaningful only when status === "running"
  logs: string[]
  error: string | null
}

export function useAgentInstallStream() {
  const [state, setState] = useState<AgentInstallStreamState>({
    status: "idle",
    progress: 0,
    logs: [],
    error: null,
  })
  const unsubRef = useRef<(() => void) | null>(null)

  const start = useCallback(async (taskId: string) => {
    setState({ status: "running", progress: 0, logs: [], error: null })

    unsubRef.current?.()

    const unsub = await subscribe<AgentInstallEvent>(
      AGENT_INSTALL_EVENT,
      (event) => {
        if (event.task_id !== taskId) return

        switch (event.kind as AgentInstallEventKind) {
          case "started":
            setState((prev) => ({ ...prev, status: "running", progress: 0 }))
            break
          case "log":
            setState((prev) => ({
              ...prev,
              logs: [...prev.logs, event.payload],
            }))
            break
          case "progress": {
            const pct = Math.min(
              100,
              Math.max(0, parseInt(event.payload, 10) || 0)
            )
            setState((prev) => ({ ...prev, progress: pct }))
            break
          }
          case "completed":
            setState((prev) => ({
              ...prev,
              status: "success",
              progress: 100,
              logs: [...prev.logs, event.payload],
            }))
            unsubRef.current?.()
            break
          case "failed":
            setState((prev) => ({
              ...prev,
              status: "failed",
              error: event.payload,
              logs: [...prev.logs, `ERROR: ${event.payload}`],
            }))
            unsubRef.current?.()
            break
        }
      }
    )

    unsubRef.current = unsub
  }, [])

  const reset = useCallback(() => {
    unsubRef.current?.()
    unsubRef.current = null
    setState({ status: "idle", progress: 0, logs: [], error: null })
  }, [])

  return { ...state, start, reset }
}
