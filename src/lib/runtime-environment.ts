"use client"

import { useEffect, useSyncExternalStore } from "react"
import { toErrorMessage } from "@/lib/app-error"
import { getTransport, onTransportChange } from "@/lib/transport"
import type { Transport } from "@/lib/transport/types"
import type { BootstrapInitEvent, BootstrapInitStatusReport } from "@/lib/types"
import { randomUUID } from "@/lib/utils"

const REPAIR_TIMEOUT_MS = 1_800_000
const STATUS_TIMEOUT_MS = 120_000
const PROGRESS_EVENT = "app://bootstrap-init"

interface RuntimeEnvironmentState {
  report: BootstrapInitStatusReport | null
  busy: "checking" | "repairing" | null
  progress: BootstrapInitEvent | null
  error: string | null
  repaired: boolean
}

const INITIAL: RuntimeEnvironmentState = {
  report: null,
  busy: null,
  progress: null,
  error: null,
  repaired: false,
}

/** 状态跟随宿主保留，切换设置页不会丢失正在执行的修复。 */
class RuntimeEnvironmentController {
  private state = INITIAL
  private listeners = new Set<() => void>()

  constructor(private transport: Transport) {}

  snapshot = () => this.state

  subscribe = (listener: () => void) => {
    this.listeners.add(listener)
    return () => this.listeners.delete(listener)
  }

  private update(patch: Partial<RuntimeEnvironmentState>) {
    this.state = { ...this.state, ...patch }
    for (const listener of this.listeners) listener()
  }

  private readStatus() {
    return this.transport.call<BootstrapInitStatusReport>(
      "bootstrap_init_status",
      {},
      { timeoutMs: STATUS_TIMEOUT_MS }
    )
  }

  refresh = async () => {
    if (this.state.busy) return
    this.update({ busy: "checking", error: null, repaired: false })
    try {
      this.update({ report: await this.readStatus() })
    } catch (error) {
      this.update({ error: toErrorMessage(error) })
    } finally {
      this.update({ busy: null })
    }
  }

  repair = async () => {
    if (this.state.busy) return
    const taskId = randomUUID()
    this.update({
      busy: "repairing",
      progress: null,
      error: null,
      repaired: false,
    })
    let unsubscribe: (() => void) | undefined
    try {
      unsubscribe = await this.watch(taskId)
      const report = await this.transport.call<BootstrapInitStatusReport>(
        "bootstrap_initialize",
        { repair: true, taskId, channel: "stable" },
        { timeoutMs: REPAIR_TIMEOUT_MS }
      )
      this.update({ report, repaired: report.phase === "ready" })
    } catch (error) {
      this.update({ error: toErrorMessage(error) })
      try {
        this.update({ report: await this.readStatus() })
      } catch {
        // 修复错误保留为主错误，检测失败不覆盖它。
      }
    } finally {
      unsubscribe?.()
      this.update({ busy: null })
    }
  }

  private async watch(taskId: string) {
    try {
      return await this.transport.subscribe<BootstrapInitEvent>(
        PROGRESS_EVENT,
        (event) => {
          if (event.taskId === taskId && this.state.busy === "repairing")
            this.update({ progress: event })
        }
      )
    } catch (error) {
      console.warn(
        "[RuntimeEnvironment] Progress unavailable:",
        toErrorMessage(error)
      )
      return undefined
    }
  }
}

const controllers = new WeakMap<Transport, RuntimeEnvironmentController>()

function controllerFor(transport: Transport) {
  let controller = controllers.get(transport)
  if (!controller) {
    controller = new RuntimeEnvironmentController(transport)
    controllers.set(transport, controller)
  }
  return controller
}

export function useRuntimeEnvironment() {
  const transport = useSyncExternalStore(
    onTransportChange,
    getTransport,
    getTransport
  )
  const controller = controllerFor(transport)
  const state = useSyncExternalStore(
    controller.subscribe,
    controller.snapshot,
    () => INITIAL
  )
  useEffect(() => {
    void controller.refresh()
  }, [controller])
  return { ...state, refresh: controller.refresh, repair: controller.repair }
}
