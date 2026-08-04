"use client"

import { Loader2 } from "lucide-react"
import { useTranslations } from "next-intl"
import { useCallback, useEffect, useRef, useState, type ReactNode } from "react"

import { Button } from "@/components/ui/button"
import { Progress } from "@/components/ui/progress"
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"
import { useIywAccount } from "@/contexts/iyw-account-context"
import { useAcpAgents } from "@/hooks/use-acp-agents"
import {
  acpDetectAgentLocalVersion,
  acpListAgents,
  acpPrepareNpxAgent,
  officecliBootstrap,
  runtimeBootstrap,
} from "@/lib/api"
import { subscribe } from "@/lib/platform"
import type { RuntimeBootstrapEvent } from "@/lib/types"
import { randomUUID } from "@/lib/utils"

const RUNTIME_BOOTSTRAP_EVENT = "app://runtime-bootstrap"

type CodexBootstrapState =
  | "idle"
  | "checking"
  | "runtime"
  | "installing"
  | "ready"
  | "error"

type RuntimePercents = Partial<Record<"node" | "git", number>>

// The bootstrap steps, in order. Kept on screen with the failure so a report
// says which one broke instead of only that something did.
type BootstrapStep = "runtime" | "registry" | "detect" | "install"

export function StartupCodexGate({ children }: { children: ReactNode }) {
  const t = useTranslations("StartupCodex")
  const { status } = useIywAccount()
  const { refresh: refreshAgents } = useAcpAgents()
  const [state, setState] = useState<CodexBootstrapState>("idle")
  const [runtimePercents, setRuntimePercents] = useState<RuntimePercents>({})
  // Why the run failed. Without this the dialog blamed the network for every
  // failure, including bugs that had nothing to do with it, and a user report
  // carried no information at all.
  const [failure, setFailure] = useState<{
    step: BootstrapStep
    detail: string
  } | null>(null)
  const runningRef = useRef(false)
  const taskIdRef = useRef(randomUUID())
  const runtimeTaskIdRef = useRef(randomUUID())
  const officeTaskIdRef = useRef(randomUUID())
  const officeBootstrapRef = useRef<Promise<void> | null>(null)
  const authenticated = status === "authenticated"
  // The dialog only appears once real installation work starts (or fails).
  // Fast probes ("checking", and a runtime bootstrap that finds everything
  // already installed) stay invisible so an up-to-date machine boots straight
  // into the workspace.
  const blocked =
    authenticated &&
    (state === "runtime" || state === "installing" || state === "error")
  const workspaceReady = authenticated && state === "ready"

  // Flip into the visible "runtime" state on the first event that proves an
  // actual download/extract is happening for this bootstrap run.
  useEffect(() => {
    if (!authenticated) return
    let disposed = false
    let unsubscribe: (() => void) | null = null
    void subscribe<RuntimeBootstrapEvent>(RUNTIME_BOOTSTRAP_EVENT, (event) => {
      if (event.task_id !== runtimeTaskIdRef.current) return
      if (!event.component) return
      setState((current) => (current === "checking" ? "runtime" : current))
      if (event.kind === "progress" && event.percent != null) {
        const component = event.component
        const percent = event.percent
        setRuntimePercents((current) => ({ ...current, [component]: percent }))
      }
    }).then((fn) => {
      if (disposed) fn()
      else unsubscribe = fn
    })
    return () => {
      disposed = true
      unsubscribe?.()
    }
  }, [authenticated])

  const bootstrapOfficeCli = useCallback(() => {
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

  const bootstrap = useCallback(async () => {
    if (runningRef.current) return
    runningRef.current = true
    setState("checking")
    setRuntimePercents({})
    setFailure(null)
    void bootstrapOfficeCli()
    // Which step is in flight, so the catch below can name it. Four different
    // backend calls funnel into one `catch`; without this the dialog cannot say
    // which of them broke.
    let step: BootstrapStep = "runtime"
    try {
      // Node/Git must exist before the Codex npx install below can run.
      const runtimeReport = await runtimeBootstrap(runtimeTaskIdRef.current)
      const failures = (
        [
          ["node", runtimeReport.node],
          ["git", runtimeReport.git],
        ] as const
      ).filter(([, component]) => component.status === "failed")
      if (failures.length > 0) {
        throw new Error(
          failures
            .map(
              ([name, component]) => `${name}: ${component.detail ?? "failed"}`
            )
            .join("\n")
        )
      }

      setState("checking")
      step = "registry"
      const agents = await acpListAgents()
      const codex = agents.find((agent) => agent.agent_type === "codex")
      if (!codex) throw new Error("Codex is missing from the Agent registry")
      step = "detect"
      const installed = await acpDetectAgentLocalVersion("codex")
      if (installed) {
        await refreshAgents()
        setState("ready")
        return
      }
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
      const detail = error instanceof Error ? error.message : String(error)
      console.error(`[StartupCodexGate] ${step} step failed:`, error)
      setFailure({ step, detail })
      setState("error")
    } finally {
      runningRef.current = false
    }
  }, [bootstrapOfficeCli, refreshAgents])

  useEffect(() => {
    if (status === "authenticated" && state === "idle") void bootstrap()
  }, [bootstrap, state, status])

  const tracked = Object.values(runtimePercents)
  const runtimePercent =
    tracked.length > 0
      ? Math.round(
          tracked.reduce((sum, percent) => sum + percent, 0) / tracked.length
        )
      : null

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
          {state !== "error" ? (
            <Progress
              value={
                state === "runtime"
                  ? (runtimePercent ?? 5)
                  : state === "installing"
                    ? 75
                    : 30
              }
              aria-label={title}
              className="h-2"
            />
          ) : null}
          {state === "error" ? (
            <div className="grid gap-4">
              {failure ? (
                <div className="grid gap-1.5">
                  <p className="text-xs font-medium text-muted-foreground">
                    {t("errorStep", { step: t(`steps.${failure.step}`) })}
                  </p>
                  <pre className="max-h-40 overflow-auto rounded-md bg-muted p-2 text-left text-xs break-all whitespace-pre-wrap">
                    {failure.detail}
                  </pre>
                </div>
              ) : null}
              <Button
                size="sm"
                className="mx-auto"
                onClick={() => void bootstrap()}
              >
                {t("retry")}
              </Button>
            </div>
          ) : null}
        </DialogContent>
      </Dialog>
    </>
  )
}
