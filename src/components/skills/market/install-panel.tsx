"use client"

import { useCallback, useEffect, useRef } from "react"
import { PackageCheck } from "lucide-react"
import { useTranslations } from "next-intl"
import { Button } from "@/components/ui/button"
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogTitle,
} from "@/components/ui/dialog"
import { AgentTargets } from "@/components/skills/market/install-agent-targets"
import {
  BusyPanel,
  InstallFailure,
  InstallPlanMetrics,
  InstallSuccess,
} from "@/components/skills/market/install-panel-parts"
import type { useSkillMarketInstall } from "@/hooks/use-skill-market-install"
import { useAcpAgents } from "@/hooks/use-acp-agents"
import { installErrorAction } from "@/lib/skill-market"
import type { AgentType } from "@/lib/types"

export interface SkillMarketInstallPanelProps {
  controller: ReturnType<typeof useSkillMarketInstall>
  pendingTarget: { name: string; version: string } | null
  onInstalled: (skillId: string, version: string) => void
  onOpenConnectors: () => void
  onClose: () => void
}

type InstallSession = ReturnType<typeof useSkillMarketInstall>["session"]
type InstallAction = ReturnType<typeof installErrorAction> | null

export function SkillMarketInstallPanel(props: SkillMarketInstallPanelProps) {
  const panel = useInstallPanelState(props)
  const { status, errorCode, plan } = panel.session
  useEffect(() => {
    if (status !== "idle")
      console.info("[SkillMarketInstallPanel] State changed", {
        status,
        errorCode,
        skillId: plan?.targetSkillId,
        version: plan?.targetVersion,
      })
  }, [status, errorCode, plan])
  if (panel.session.status === "idle") return null
  return <InstallPanelDialog target={props.pendingTarget} panel={panel} />
}

function useInstallPanelState(props: SkillMarketInstallPanelProps) {
  const { session, start, retry, reset } = props.controller
  const onInstalledRef = useRef(props.onInstalled)
  const notifiedRef = useRef(false)

  useEffect(() => {
    onInstalledRef.current = props.onInstalled
  }, [props.onInstalled])

  useEffect(() => {
    if (session.status === "done" && session.plan && !notifiedRef.current) {
      notifiedRef.current = true
      onInstalledRef.current(
        session.plan.targetSkillId,
        session.plan.targetVersion
      )
    }
    if (session.status !== "done") notifiedRef.current = false
  }, [session])

  const errorAction = session.errorCode
    ? installErrorAction(session.errorCode)
    : null
  const close = useCallback(() => {
    reset()
    props.onClose()
  }, [props, reset])
  const openConnectors = useCallback(() => {
    close()
    props.onOpenConnectors()
  }, [close, props])

  return {
    session,
    errorAction,
    start: (agentTypes: AgentType[]) => {
      console.info("[SkillMarketInstallPanel] Installing for detected agents", {
        skillId: session.plan?.targetSkillId,
        version: session.plan?.targetVersion,
        agentTypes,
      })
      void start(agentTypes)
    },
    retry,
    close,
    openConnectors,
  }
}

type InstallPanelState = ReturnType<typeof useInstallPanelState>

function InstallPanelDialog({
  target,
  panel,
}: {
  target: SkillMarketInstallPanelProps["pendingTarget"]
  panel: InstallPanelState
}) {
  const canClose = !["resolving", "running"].includes(panel.session.status)
  return (
    <Dialog
      open
      onOpenChange={(open) => {
        if (!open && canClose) panel.close()
      }}
    >
      <DialogContent
        className="w-full max-w-xl gap-0 overflow-y-auto rounded-lg p-0"
        showCloseButton={canClose}
        onInteractOutside={(event) => event.preventDefault()}
      >
        <InstallPanelHeader target={target} />
        <InstallPanelBody
          session={panel.session}
          errorAction={panel.errorAction}
          onStart={panel.start}
          onRetry={panel.retry}
          onClose={panel.close}
          onOpenConnectors={panel.openConnectors}
        />
      </DialogContent>
    </Dialog>
  )
}

function InstallPanelHeader({
  target,
}: {
  target: SkillMarketInstallPanelProps["pendingTarget"]
}) {
  const t = useTranslations("SkillMarketV2")
  return (
    <div className="flex items-start gap-3 border-b px-4 py-3 pr-14">
      <span className="flex size-8 shrink-0 items-center justify-center rounded-md bg-primary/10 text-primary">
        <PackageCheck className="size-4" aria-hidden="true" />
      </span>
      <div className="min-w-0 flex-1">
        <DialogTitle className="truncate text-sm font-semibold">
          {target?.name ?? t("install.title")}
        </DialogTitle>
        <DialogDescription className="mt-0.5 text-[11px] text-muted-foreground">
          {target ? `v${target.version}` : ""}
        </DialogDescription>
      </div>
    </div>
  )
}

interface InstallPanelBodyProps {
  session: InstallSession
  errorAction: InstallAction
  onStart: (agentTypes: AgentType[]) => void
  onRetry: () => void
  onClose: () => void
  onOpenConnectors: () => void
}

function InstallPanelBody(props: InstallPanelBodyProps) {
  const { session } = props
  const includesConnectors = Boolean(
    session.plan?.items.some((item) =>
      item.plugin?.components.some(
        (component) => component.type === "connector"
      )
    )
  )
  const requiresAgentTargets = Boolean(
    session.plan?.items.some(
      (item) =>
        item.packageType !== "plugin" ||
        item.plugin?.components.some((component) => component.type === "skill")
    )
  )
  if (session.status === "resolving" || session.status === "running")
    return <BusyPanel session={session} />
  if (session.status === "confirming" && session.plan) {
    return (
      <InstallConfirmation
        plan={session.plan}
        includesConnectors={includesConnectors}
        requiresAgentTargets={requiresAgentTargets}
        onStart={props.onStart}
      />
    )
  }
  if (session.status === "done")
    return (
      <InstallSuccess
        includesConnectors={includesConnectors}
        onClose={props.onClose}
        onOpenConnectors={props.onOpenConnectors}
      />
    )
  if (session.status === "failed")
    return (
      <InstallFailure
        errorMessage={session.errorMessage}
        errorAction={props.errorAction}
        onRetry={props.onRetry}
        onClose={props.onClose}
      />
    )
  return null
}

interface InstallConfirmationProps {
  plan: NonNullable<ReturnType<typeof useSkillMarketInstall>["session"]["plan"]>
  includesConnectors: boolean
  requiresAgentTargets: boolean
  onStart: (agentTypes: AgentType[]) => void
}

function InstallConfirmation({
  plan,
  includesConnectors,
  requiresAgentTargets,
  onStart,
}: InstallConfirmationProps) {
  const t = useTranslations("SkillMarketV2")
  const { agents, fresh } = useAcpAgents()
  const targets =
    fresh && requiresAgentTargets
      ? agents.filter((agent) => agent.enabled && agent.installed_version)
      : []
  return (
    <div className="space-y-4 p-4">
      <InstallPlanMetrics
        totalBytes={plan.totalBytes}
        dependencyCount={plan.dependencyCount}
        targetCount={targets.length}
      />
      {includesConnectors ? (
        <p className="border-l-2 border-amber-500/50 bg-amber-500/5 px-3 py-2 text-xs text-muted-foreground">
          {t("install.connectorsOff")}
        </p>
      ) : null}
      {requiresAgentTargets ? (
        <AgentTargets count={targets.length} fresh={fresh} />
      ) : null}
      <InstallConfirmationActions
        targetCount={targets.length}
        includesConnectors={includesConnectors}
        requiresAgentTargets={requiresAgentTargets}
        onStart={() => onStart(targets.map((agent) => agent.agent_type))}
      />
    </div>
  )
}

function InstallConfirmationActions({
  targetCount,
  includesConnectors,
  requiresAgentTargets,
  onStart,
}: {
  targetCount: number
  includesConnectors: boolean
  requiresAgentTargets: boolean
  onStart: () => void
}) {
  const t = useTranslations("SkillMarketV2")
  return (
    <div className="flex justify-end">
      <Button
        size="sm"
        disabled={requiresAgentTargets && targetCount === 0}
        onClick={onStart}
      >
        {t(
          includesConnectors
            ? "install.installPlugin"
            : "install.installAndEnable"
        )}
      </Button>
    </div>
  )
}
