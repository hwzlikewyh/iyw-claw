"use client"

import { Loader2 } from "lucide-react"
import { useTranslations } from "next-intl"
import { useCallback, useEffect, useRef, useState, type ReactNode } from "react"

import { Progress } from "@/components/ui/progress"
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"
import { OverlayWindowControls } from "@/components/layout/overlay-window-controls"
import { useIywAccount } from "@/contexts/iyw-account-context"
import { useAcpAgents } from "@/hooks/use-acp-agents"
import { officecliBootstrap } from "@/lib/api"
import { isLocalDesktop, subscribe } from "@/lib/platform"
import type { BootstrapComponentStatus, BootstrapInitEvent } from "@/lib/types"
import { randomUUID } from "@/lib/utils"
import {
  executeCodexBootstrap,
  type BootstrapStep,
} from "./startup-codex-bootstrap"
import {
  StartupFailureDetails,
  StartupRuntimeStatus,
  updateRuntimeComponent,
} from "./startup-runtime-status"

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
  const [components, setComponents] = useState<BootstrapComponentStatus[]>([])
  const [message, setMessage] = useState("")
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
      setMessage(event.message)
      if (event.component) {
        setComponents((current) => updateRuntimeComponent(current, event))
      }
      const activePhase = [
        "downloading",
        "staging",
        "activating",
        "health_check",
      ].includes(event.phase)
      if (!activePhase) return
      setState((current) => (current === "checking" ? "runtime" : current))
      if (event.phase === "downloading" && event.total && event.total > 0) {
        setBootstrapPercent(
          Math.min(
            100,
            Math.max(0, ((event.downloaded ?? 0) / event.total) * 100)
          )
        )
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
      setMessage("")
      const controller = new AbortController()
      controllerRef.current = controller
      const visibilityTimer = setTimeout(() => {
        setState((current) => (current === "checking" ? "runtime" : current))
      }, CHECKING_VISIBILITY_DELAY_MS)
      void bootstrapOfficeCli()
      let step: BootstrapStep = "registry"
      try {
        const agents = await acpListAgents().catch((error) => {
          console.warn(
            "[StartupCodexGate] Agent registry unavailable; continuing fail-closed:",
            error
          )
          return []
        })
        const codex = agents.find((agent) => agent.agent_type === "codex")
        if (!codex) {
          await refreshAgents()
          setState("ready")
          return
        }

        step = "runtime"
        const runtimeReport = await prepareStartupRuntime({
          repair,
          taskId: runtimeTaskIdRef.current,
          signal: controller.signal,
          onStatus: (report) => {
            setComponents(report.components)
            if (report.writerBusy) setMessage(t("waitingForWriter"))
          },
        })
        const requiredComponents = ["node", "git", "uv"]
        const components = new Map(
          runtimeReport.components.map((component) => [
            component.componentId,
            component,
          ])
        )
        const failures = requiredComponents.flatMap((componentId) => {
          const component = components.get(componentId)
          if (!component) return [`${componentId}: ${t("componentPending")}`]
          if (!component.installed || !component.active) {
            return [`${componentId}: ${component.lastError ?? component.phase}`]
          }
          return []
        })
        if (failures.length > 0) {
          throw new Error(failures.join("\n"))
        }
        step = "detect"
        const installed =
          codex.installed_version ?? (await acpDetectAgentLocalVersion("codex"))
        if (installed) {
          await refreshAgents()
          setState("ready")
          return
        }
        if (isLocalDesktop()) throw new Error(t("repairCore"))
        setState("installing")
        step = "install"
        await acpPrepareNpxAgent(
          "codex",
          codex.registry_version,
          taskIdRef.current,
          false
        )
        await refreshAgents()
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
    if (status === "authenticated" && state === "idle") void bootstrap(false)
  }, [bootstrap, state, status])

  const title =
    state === "runtime"
      ? t("runtimeTitle")
      : state === "installing"
        ? t("installingTitle")
        : t("checkingTitle")
  const description =
    state === "runtime"
      ? t("runtimeDescription")
      : state === "installing"
        ? t("installingDescription")
        : t("checkingDescription")

  return (
    <>
      {workspaceReady ? children : null}
      <OverlayWindowControls visible={blocked} />
      <Dialog open={blocked} onOpenChange={() => {}}>
        <DialogContent
          className="max-w-md rounded-lg"
          showCloseButton={false}
          onEscapeKeyDown={(event) => event.preventDefault()}
          onPointerDownOutside={(event) => event.preventDefault()}
          onInteractOutside={(event) => event.preventDefault()}
        >
          <DialogHeader className="text-center">
            {state !== "error" ? (
              <Loader2 className="mx-auto mb-2 h-7 w-7 animate-spin text-muted-foreground" />
            ) : null}
            <DialogTitle>
              {state === "error" ? t("errorTitle") : title}
            </DialogTitle>
            <DialogDescription>
              {state === "error" ? t("errorDescription") : description}
            </DialogDescription>
          </DialogHeader>
          {components.length > 0 ? (
            <StartupRuntimeStatus components={components} />
          ) : null}
          {message && state !== "error" ? (
            <p role="status" className="text-xs text-muted-foreground">
              {message}
            </p>
          ) : null}
          {state !== "error" ? (
            <Progress
              value={
                state === "runtime"
                  ? (bootstrapPercent ?? 5)
                  : state === "installing"
                    ? 75
                    : 30
              }
              aria-label={title}
              className="h-2"
            />
          ) : null}
          {state === "error" && failure ? (
            <StartupFailureDetails
              step={t(`steps.${failure.step}`)}
              detail={failure.detail}
              onRetry={() => void bootstrap(true)}
            />
          ) : null}
        </DialogContent>
      </Dialog>
    </>
  )
}
