"use client"

import { useCallback, useEffect, useRef, useState } from "react"
import { PackageCheck, X } from "lucide-react"
import { useTranslations } from "next-intl"
import { Button } from "@/components/ui/button"
import { AgentTargets } from "@/components/skills/market/install-agent-targets"
import {
  BusyPanel,
  InstallFailure,
  InstallPlanMetrics,
  InstallSuccess,
} from "@/components/skills/market/install-panel-parts"
import type { useSkillMarketInstall } from "@/hooks/use-skill-market-install"
import { installErrorAction } from "@/lib/skill-market"
import type { AgentType } from "@/lib/types"

export interface SkillMarketInstallPanelProps {
  controller: ReturnType<typeof useSkillMarketInstall>
  pendingTarget: { name: string; version: string } | null
  onInstalled: (skillId: string, version: string) => void
  onClose: () => void
}

type InstallSession = ReturnType<typeof useSkillMarketInstall>["session"]
type InstallAction = ReturnType<typeof installErrorAction> | null

export function SkillMarketInstallPanel(props: SkillMarketInstallPanelProps) {
  const panel = useInstallPanelState(props)
  if (panel.session.status === "idle") return null
  return <InstallPanelDialog target={props.pendingTarget} panel={panel} />
}

function useInstallPanelState(props: SkillMarketInstallPanelProps) {
  const { session, start, retry, reset } = props.controller
  const [selected, setSelected] = useState<Set<AgentType>>(new Set())
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
    setSelected(new Set())
    props.onClose()
  }, [props, reset])

  return {
    session,
    selected,
    errorAction,
    setSelected,
    start: () => void start([...selected]),
    retry,
    close,
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
  const t = useTranslations("SkillMarketV2")

  return (
    <div className="fixed inset-0 z-50 flex items-end justify-center bg-black/25 p-4 backdrop-blur-[1px] sm:items-center">
      <div
        role="dialog"
        aria-modal="true"
        aria-label={t("install.title")}
        className="w-full max-w-xl rounded-lg border bg-background shadow-2xl"
      >
        <InstallPanelHeader target={target} panel={panel} />
        <InstallPanelBody
          session={panel.session}
          selected={panel.selected}
          errorAction={panel.errorAction}
          onSelectedChange={panel.setSelected}
          onStart={panel.start}
          onRetry={panel.retry}
          onClose={panel.close}
        />
      </div>
    </div>
  )
}

function InstallPanelHeader({
  target,
  panel,
}: {
  target: SkillMarketInstallPanelProps["pendingTarget"]
  panel: InstallPanelState
}) {
  const t = useTranslations("SkillMarketV2")
  const canClose = !["resolving", "running"].includes(panel.session.status)
  return (
    <div className="flex items-start gap-3 border-b px-4 py-3">
      <span className="flex size-8 shrink-0 items-center justify-center rounded-md bg-primary/10 text-primary">
        <PackageCheck className="size-4" aria-hidden="true" />
      </span>
      <div className="min-w-0 flex-1">
        <h3 className="truncate text-sm font-semibold">
          {target?.name ?? t("install.title")}
        </h3>
        <p className="mt-0.5 text-[11px] text-muted-foreground">
          {target ? `v${target.version}` : ""}
        </p>
      </div>
      {canClose ? (
        <Button
          size="icon-sm"
          variant="ghost"
          aria-label={t("install.close")}
          title={t("install.close")}
          onClick={panel.close}
        >
          <X className="size-3.5" aria-hidden="true" />
        </Button>
      ) : null}
    </div>
  )
}

interface InstallPanelBodyProps {
  session: InstallSession
  selected: Set<AgentType>
  errorAction: InstallAction
  onSelectedChange: (next: Set<AgentType>) => void
  onStart: () => void
  onRetry: () => void
  onClose: () => void
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
  if (session.status === "resolving") return <BusyPanel message="resolving" />
  if (session.status === "running")
    return <BusyPanel message="installingReal" />
  if (session.status === "confirming" && session.plan) {
    return (
      <InstallConfirmation
        plan={session.plan}
        selected={props.selected}
        includesConnectors={includesConnectors}
        requiresAgentTargets={requiresAgentTargets}
        onSelectedChange={props.onSelectedChange}
        onStart={props.onStart}
      />
    )
  }
  if (session.status === "done")
    return (
      <InstallSuccess
        includesConnectors={includesConnectors}
        onClose={props.onClose}
      />
    )
  if (session.status === "failed")
    return (
      <InstallFailure
        errorMessage={session.errorMessage}
        errorAction={props.errorAction}
        onRetry={props.onRetry}
      />
    )
  return null
}

function InstallConfirmation({
  plan,
  selected,
  includesConnectors,
  requiresAgentTargets,
  onSelectedChange,
  onStart,
}: {
  plan: NonNullable<ReturnType<typeof useSkillMarketInstall>["session"]["plan"]>
  selected: Set<AgentType>
  includesConnectors: boolean
  requiresAgentTargets: boolean
  onSelectedChange: (next: Set<AgentType>) => void
  onStart: () => void
}) {
  const t = useTranslations("SkillMarketV2")
  return (
    <div className="space-y-4 p-4">
      <InstallPlanMetrics
        totalBytes={plan.totalBytes}
        dependencyCount={plan.dependencyCount}
        targetCount={selected.size}
      />
      {includesConnectors ? (
        <p className="border-l-2 border-amber-500/50 bg-amber-500/5 px-3 py-2 text-xs text-muted-foreground">
          {t("install.connectorsOff")}
        </p>
      ) : null}
      {requiresAgentTargets ? (
        <AgentTargets selected={selected} onChange={onSelectedChange} />
      ) : null}
      <InstallConfirmationActions
        selectedCount={selected.size}
        includesConnectors={includesConnectors}
        requiresAgentTargets={requiresAgentTargets}
        onStart={onStart}
      />
    </div>
  )
}

function InstallConfirmationActions({
  selectedCount,
  includesConnectors,
  requiresAgentTargets,
  onStart,
}: {
  selectedCount: number
  includesConnectors: boolean
  requiresAgentTargets: boolean
  onStart: () => void
}) {
  const t = useTranslations("SkillMarketV2")
  return (
    <div className="flex items-center justify-between gap-3">
      {requiresAgentTargets ? (
        <p className="text-[10px] text-muted-foreground">
          {t("install.profileRule")}
        </p>
      ) : (
        <span />
      )}
      <Button
        size="sm"
        disabled={requiresAgentTargets && selectedCount === 0}
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
