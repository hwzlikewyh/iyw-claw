"use client"

import { useTranslations } from "next-intl"
import { useCallback, useEffect, useRef, useState, type ReactNode } from "react"

import { Dialog, DialogContent } from "@/components/ui/dialog"
import { OverlayWindowControls } from "@/components/layout/overlay-window-controls"
import { useIywAccount } from "@/contexts/iyw-account-context"
import { useAcpAgents } from "@/hooks/use-acp-agents"
import { officecliBootstrap } from "@/lib/api"
import { isLocalDesktop, subscribe } from "@/lib/platform"
import type { BootstrapInitEvent } from "@/lib/types"
import { randomUUID } from "@/lib/utils"
import {
  type BootstrapStep,
  executeCodexBootstrap,
} from "./startup-codex-bootstrap"
import { StartupInitializationPanel } from "./startup-initialization-panel"

const BOOTSTRAP_INIT_EVENT = "app://bootstrap-init"
const CHECKING_VISIBILITY_DELAY_MS = 800

type CodexBootstrapState =
  | "idle"
  | "checking"
  | "runtime"
  | "installing"
  | "ready"
  | "error"

export function StartupCodexGate({ children }: { children: ReactNode }) {
  const t = useTranslations("StartupCodex")
  const { status } = useIywAccount()
  const { refresh: refreshAgents } = useAcpAgents()
  const [state, setState] = useState<CodexBootstrapState>("idle")
  const [bootstrapPercent, setBootstrapPercent] = useState<number | null>(null)
  const [stage, setStage] = useState(0)
  const [waiting, setWaiting] = useState(false)
  const controllerRef = useRef<AbortController | null>(null)
  const [failure, setFailure] = useState<{
    step: BootstrapStep
    detail: string
  } | null>(null)
  const runningRef = useRef(false)
  const taskIdRef = useRef(randomUUID())
  const runtimeTaskIdRef = useRef(randomUUID())
  const officeTaskIdRef = useRef(randomUUID())
  const officeBootstrapRef = useRef<Promise<void> | null>(null)
  const workspaceReadyOnceRef = useRef(false)
  const authenticated = status === "authenticated"
  const shouldCheck = authenticated || isLocalDesktop()
  const blocked =
    shouldCheck &&
    (state === "runtime" || state === "installing" || state === "error")
  if (shouldCheck && state === "ready") {
    workspaceReadyOnceRef.current = true
  }
  const workspaceReady = workspaceReadyOnceRef.current

  useEffect(() => () => controllerRef.current?.abort(), [])

  useEffect(() => {
    if (!shouldCheck) return
    let disposed = false
    let unsubscribe: (() => void) | null = null
    void subscribe<BootstrapInitEvent>(BOOTSTRAP_INIT_EVENT, (event) => {
      if (event.taskId !== runtimeTaskIdRef.current) return
      if (!runningRef.current) return
      const activePhase = [
        "downloading",
        "staging",
        "activating",
        "health_check",
        "verifying",
        "ready",
      ].includes(event.phase)
      if (!activePhase) return
      setState((current) => (current === "checking" ? "runtime" : current))
      const completing = ["activating", "health_check", "ready"].includes(
        event.phase
      )
      setStage((current) => Math.max(current, completing ? 2 : 1))
      if (typeof event.percent === "number" && Number.isFinite(event.percent)) {
        const percent = Math.max(0, Math.min(95, Math.floor(event.percent)))
        setBootstrapPercent((current) => Math.max(current ?? 0, percent))
      }
    })
      .then((fn) => {
        if (disposed) fn()
        else unsubscribe = fn
      })
      .catch((error) => {
        console.warn("[StartupCodexGate] Progress subscription failed:", error)
      })
    return () => {
      disposed = true
      unsubscribe?.()
    }
  }, [shouldCheck])

  const bootstrapOfficeCli = useCallback(() => {
    if (isLocalDesktop()) return Promise.resolve()
    if (officeBootstrapRef.current) return officeBootstrapRef.current
    officeBootstrapRef.current = (async () => {
      const report = await officecliBootstrap(officeTaskIdRef.current)
      if (report.errors.length > 0) {
        throw new Error(report.errors.join("\n"))
      }
    })().catch((error) => {
      console.warn("[StartupCodexGate] OfficeCLI bootstrap failed:", error)
    })
    return officeBootstrapRef.current
  }, [])

  const bootstrap = useCallback(
    async (repair = false) => {
      if (runningRef.current) return
      runningRef.current = true
      runtimeTaskIdRef.current = randomUUID()
      setState("checking")
      setBootstrapPercent(null)
      setFailure(null)
      setStage(0)
      setWaiting(false)
      const controller = new AbortController()
      controllerRef.current = controller
      const visibilityTimer = setTimeout(() => {
        setState((current) => (current === "checking" ? "runtime" : current))
      }, CHECKING_VISIBILITY_DELAY_MS)
      void bootstrapOfficeCli()
      let step: BootstrapStep = "registry"
      try {
        await executeCodexBootstrap({
          repair,
          runtimeTaskId: runtimeTaskIdRef.current,
          installTaskId: taskIdRef.current,
          signal: controller.signal,
          messages: {
            componentPending: t("componentPending"),
            repairCore: t("repairCore"),
          },
          onStep: (next) => {
            step = next
            if (next === "detect") setStage(2)
          },
          onStatus: (report) => {
            setWaiting(report.writerBusy)
          },
          onInstalling: () => {
            setState("installing")
            setStage(1)
          },
          refreshAgents,
        })
        setBootstrapPercent(100)
        setStage(2)
        setState("ready")
      } catch (error) {
        if (controller.signal.aborted) {
          setState("idle")
          return
        }
        const detail = error instanceof Error ? error.message : String(error)
        console.error(`[StartupCodexGate] ${step} step failed:`, error)
        setFailure({ step, detail })
        setState("error")
      } finally {
        clearTimeout(visibilityTimer)
        runningRef.current = false
      }
    },
    [bootstrapOfficeCli, refreshAgents, t]
  )

  useEffect(() => {
    if (shouldCheck && state === "idle") void bootstrap(false)
  }, [bootstrap, state, shouldCheck])

  return (
    <>
      {workspaceReady ? children : null}
      <OverlayWindowControls visible={blocked} />
      <Dialog open={blocked} onOpenChange={() => {}}>
        <DialogContent
          className="max-h-[calc(100dvh-6rem)] gap-0 overflow-y-auto rounded-lg p-0 sm:max-w-[560px]"
          showCloseButton={false}
          onEscapeKeyDown={(event) => event.preventDefault()}
          onPointerDownOutside={(event) => event.preventDefault()}
          onInteractOutside={(event) => event.preventDefault()}
        >
          <StartupInitializationPanel
            stage={stage}
            percent={bootstrapPercent}
            waiting={waiting}
            failure={
              state === "error" ? (failure?.detail ?? t("setupFailed")) : null
            }
            onRetry={() => void bootstrap(true)}
          />
        </DialogContent>
      </Dialog>
    </>
  )
}
