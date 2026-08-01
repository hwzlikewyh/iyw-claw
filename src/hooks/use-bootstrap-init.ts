import { useCallback, useRef, useState } from "react"

import { bootstrapInitStatus } from "@/lib/api"
import { subscribe } from "@/lib/platform"
import type { BootstrapInitEvent } from "@/lib/types"

export const BOOTSTRAP_INIT_EVENT = "app://bootstrap-init"

export interface BootstrapInitViewState {
  phase: string
  component: string | null
  downloaded: number | null
  total: number | null
  rateBps: number | null
  etaSecs: number | null
  message: string
  offline: boolean
  writerBusy: boolean
  lastError: string | null
}

const IDLE: BootstrapInitViewState = {
  phase: "not_started",
  component: null,
  downloaded: null,
  total: null,
  rateBps: null,
  etaSecs: null,
  message: "",
  offline: false,
  writerBusy: false,
  lastError: null,
}

/**
 * 订阅 `app://bootstrap-init` 进度事件并轮询初始化状态。
 * 仅做展示，不触发初始化；初始化由门禁流程调用 `bootstrapInitialize`。
 */
export function useBootstrapInit(taskId: string) {
  const [state, setState] = useState<BootstrapInitViewState>(IDLE)
  const unsubRef = useRef<(() => void) | null>(null)

  const start = useCallback(async () => {
    unsubRef.current?.()
    const unsub = await subscribe<BootstrapInitEvent>(
      BOOTSTRAP_INIT_EVENT,
      (event) => {
        if (event.task_id !== taskId) return
        setState((prev) => ({
          ...prev,
          phase: event.phase,
          component: event.component ?? prev.component,
          downloaded: event.downloaded ?? prev.downloaded,
          total: event.total ?? prev.total,
          rateBps: event.rate_bps ?? prev.rateBps,
          etaSecs: event.eta_secs ?? prev.etaSecs,
          message: event.message || prev.message,
          lastError:
            event.phase === "blocked" || event.phase === "degraded"
              ? event.message || prev.lastError
              : prev.lastError,
        }))
      }
    )
    unsubRef.current = unsub
  }, [taskId])

  const refreshStatus = useCallback(async () => {
    try {
      const report = await bootstrapInitStatus()
      setState((prev) => ({
        ...prev,
        phase: report.phase,
        offline: report.offline,
        writerBusy: report.writer_busy,
        lastError:
          report.phase === "blocked" || report.phase === "degraded"
            ? (report.components.find((item) => item.last_error)?.last_error ??
              prev.lastError)
            : prev.lastError,
      }))
      return report
    } catch {
      return null
    }
  }, [])

  const stop = useCallback(() => {
    unsubRef.current?.()
    unsubRef.current = null
  }, [])

  const percent =
    state.total && state.total > 0
      ? Math.min(100, Math.max(0, ((state.downloaded ?? 0) / state.total) * 100))
      : null

  return {
    state,
    percent,
    start,
    stop,
    refreshStatus,
  }
}
