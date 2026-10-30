"use client"

import { useEffect, useRef, useState, type ReactNode } from "react"
import { useTranslations } from "next-intl"
import { Loader2 } from "lucide-react"
import { Dialog, DialogContent } from "@/components/ui/dialog"
import { useRuntimeEnvironment } from "@/lib/runtime-environment"
import type { BootstrapInitStatusReport } from "@/lib/types"
import { DesktopRuntimeMonitor } from "./desktop-runtime-monitor"
import { StartupInitializationPanel } from "./startup-initialization-panel"

type SetupPhase = "checking" | "initializing" | "ready"
type EnvironmentState = ReturnType<typeof useRuntimeEnvironment>

const REQUIRED_COMPONENTS = ["node", "git", "uv", "builtin-agent"]
const MAX_PENDING_PERCENT = 99

function setupFailure(report: BootstrapInitStatusReport, pending: string) {
  const components = new Map(
    report.components.map((component) => [component.componentId, component])
  )
  const environmentError = components.get("environment")?.lastError
  if (environmentError) return environmentError
  return REQUIRED_COMPONENTS.flatMap((id) => {
    const component = components.get(id)
    return component?.installed && component.active
      ? []
      : [`${id}: ${component?.lastError ?? pending}`]
  }).join("\n")
}

function useMacosSetup(state: EnvironmentState) {
  const t = useTranslations("StartupCodex")
  const [phase, setPhase] = useState<SetupPhase>("checking")
  const started = useRef(false)
  const { report, busy, error, repair } = state
  let nextPhase = phase
  if (report && !busy && !error) {
    if (phase === "checking") {
      nextPhase = report.components.some(
        (component) => component.componentId === "environment"
      )
        ? "initializing"
        : "ready"
    } else if (
      phase === "initializing" &&
      !setupFailure(report, t("componentPending"))
    ) {
      nextPhase = "ready"
    }
  }
  if (nextPhase !== phase) setPhase(nextPhase)
  useEffect(() => {
    if (phase !== "initializing" || busy || started.current) return
    started.current = true
    console.info("[MacosStartup] Preparing initial runtime environment")
    void repair()
  }, [repair, phase, busy])
  return phase
}

function setupStage(state: EnvironmentState, phase: SetupPhase) {
  if (phase === "checking") return 0
  return ["activating", "verifying", "health_check", "ready"].includes(
    state.progress?.phase ?? ""
  )
    ? 2
    : 1
}

export function MacosStartupEnvironmentGate({
  children,
}: {
  children: ReactNode
}) {
  const t = useTranslations("RuntimeEnvironmentSettings")
  const state = useRuntimeEnvironment()
  const phase = useMacosSetup(state)
  if (phase === "checking" && !state.error) {
    return (
      <div
        role="status"
        className="flex h-dvh items-center justify-center gap-2 text-sm text-muted-foreground"
      >
        <Loader2
          className="size-4 animate-spin motion-reduce:animate-none"
          aria-hidden
        />
        {t("checking")}
      </div>
    )
  }
  if (phase === "ready") {
    return (
      <>
        {children}
        <DesktopRuntimeMonitor />
      </>
    )
  }
  return <MacosSetupDialog state={state} phase={phase} />
}

function MacosSetupDialog({
  state,
  phase,
}: {
  state: EnvironmentState
  phase: SetupPhase
}) {
  const t = useTranslations("StartupCodex")
  const failure =
    state.error ||
    (!state.busy && phase === "initializing" && state.report
      ? setupFailure(state.report, t("componentPending"))
      : null)
  const percent = state.progress?.percent
  return (
    <Dialog open onOpenChange={() => {}}>
      <DialogContent
        className="max-h-[calc(100dvh-6rem)] gap-0 overflow-y-auto rounded-lg p-0 sm:max-w-[560px]"
        showCloseButton={false}
        onEscapeKeyDown={(event) => event.preventDefault()}
        onPointerDownOutside={(event) => event.preventDefault()}
        onInteractOutside={(event) => event.preventDefault()}
      >
        <StartupInitializationPanel
          stage={setupStage(state, phase)}
          percent={
            typeof percent === "number" && Number.isFinite(percent)
              ? Math.max(0, Math.min(MAX_PENDING_PERCENT, percent))
              : null
          }
          waiting={!!state.report?.writerBusy}
          failure={failure}
          onRetry={() =>
            void (phase === "checking" ? state.refresh() : state.repair())
          }
        />
      </DialogContent>
    </Dialog>
  )
}
