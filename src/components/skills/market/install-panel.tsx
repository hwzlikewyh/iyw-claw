"use client"

import { useEffect, useRef, useState } from "react"
import {
  AlertTriangle,
  Check,
  Loader2,
  PackageCheck,
  RotateCcw,
  X,
} from "lucide-react"
import { useTranslations } from "next-intl"
import { Button } from "@/components/ui/button"
import { AgentTargets } from "@/components/skills/market/install-agent-targets"
import type { useSkillMarketInstall } from "@/hooks/use-skill-market-install"
import {
  formatSkillBytes,
  installErrorAction,
  type SkillMarketTranslator,
} from "@/lib/skill-market"
import type { AgentType } from "@/lib/types"

export interface SkillMarketInstallPanelProps {
  controller: ReturnType<typeof useSkillMarketInstall>
  pendingTarget: { name: string; version: string } | null
  onInstalled: (skillId: string, version: string) => void
  onClose: () => void
}

export function SkillMarketInstallPanel(props: SkillMarketInstallPanelProps) {
  const t = useTranslations("SkillMarketV2") as unknown as SkillMarketTranslator
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

  if (session.status === "idle") return null
  const errorAction = session.errorCode
    ? installErrorAction(session.errorCode)
    : null
  const close = () => {
    reset()
    setSelected(new Set())
    props.onClose()
  }

  return (
    <div className="fixed inset-0 z-50 flex items-end justify-center bg-black/25 p-4 backdrop-blur-[1px] sm:items-center">
      <div
        role="dialog"
        aria-modal="true"
        aria-label={t("install.title")}
        className="w-full max-w-xl rounded-lg border bg-background shadow-2xl"
      >
        <div className="flex items-start gap-3 border-b px-4 py-3">
          <span className="flex size-8 shrink-0 items-center justify-center rounded-md bg-primary/10 text-primary">
            <PackageCheck className="size-4" aria-hidden="true" />
          </span>
          <div className="min-w-0 flex-1">
            <h3 className="truncate text-sm font-semibold">
              {props.pendingTarget?.name ?? t("install.title")}
            </h3>
            <p className="mt-0.5 text-[11px] text-muted-foreground">
              {props.pendingTarget ? `v${props.pendingTarget.version}` : ""}
            </p>
          </div>
          {!["resolving", "running"].includes(session.status) ? (
            <Button
              size="icon-sm"
              variant="ghost"
              aria-label={t("install.close")}
              title={t("install.close")}
              onClick={close}
            >
              <X className="size-3.5" aria-hidden="true" />
            </Button>
          ) : null}
        </div>

        <InstallPanelBody
          session={session}
          selected={selected}
          errorAction={errorAction}
          onSelectedChange={setSelected}
          onStart={() => void start([...selected])}
          onRetry={retry}
          onClose={close}
        />
      </div>
    </div>
  )
}

function InstallPanelBody({
  session,
  selected,
  errorAction,
  onSelectedChange,
  onStart,
  onRetry,
  onClose,
}: {
  session: ReturnType<typeof useSkillMarketInstall>["session"]
  selected: Set<AgentType>
  errorAction: ReturnType<typeof installErrorAction> | null
  onSelectedChange: (next: Set<AgentType>) => void
  onStart: () => void
  onRetry: () => void
  onClose: () => void
}) {
  const t = useTranslations("SkillMarketV2") as unknown as SkillMarketTranslator
  if (session.status === "resolving")
    return (
      <div className="p-4">
        <InstallBusy label={t("install.resolving")} />
      </div>
    )
  if (session.status === "running")
    return (
      <div className="p-4">
        <InstallBusy label={t("install.installingReal")} />
      </div>
    )
  if (session.status === "confirming" && session.plan) {
    return (
      <InstallConfirmation
        plan={session.plan}
        selected={selected}
        onSelectedChange={onSelectedChange}
        onStart={onStart}
      />
    )
  }
  if (session.status === "done") {
    return (
      <div className="p-4">
        <ResultState
          icon={<Check className="size-5 text-emerald-500" />}
          title={t("install.done")}
        >
          <Button size="sm" onClick={onClose}>
            {t("install.doneClose")}
          </Button>
        </ResultState>
      </div>
    )
  }
  if (session.status === "failed") {
    return (
      <div className="p-4">
        <ResultState
          icon={<AlertTriangle className="size-5 text-destructive" />}
          title={t("install.failed")}
          detail={session.errorMessage}
        >
          <Button size="sm" onClick={onRetry}>
            <RotateCcw className="size-3.5" />
            {t("install.actionRetry")}
          </Button>
          {errorAction && errorAction !== "retry" ? (
            <span className="text-[10px] text-muted-foreground">
              {t(`install.action${capitalize(errorAction)}`)}
            </span>
          ) : null}
        </ResultState>
      </div>
    )
  }
  return null
}

function InstallConfirmation({
  plan,
  selected,
  onSelectedChange,
  onStart,
}: {
  plan: NonNullable<ReturnType<typeof useSkillMarketInstall>["session"]["plan"]>
  selected: Set<AgentType>
  onSelectedChange: (next: Set<AgentType>) => void
  onStart: () => void
}) {
  const t = useTranslations("SkillMarketV2")
  return (
    <div className="space-y-4 p-4">
      <div className="grid grid-cols-3 gap-3 border-b pb-3 text-xs">
        <InstallMetric
          label={t("install.downloadSize")}
          value={formatSkillBytes(plan.totalBytes)}
        />
        <InstallMetric
          label={t("detail.dependencies")}
          value={String(plan.dependencyCount)}
        />
        <InstallMetric
          label={t("install.targets")}
          value={String(selected.size)}
        />
      </div>
      <AgentTargets selected={selected} onChange={onSelectedChange} />
      <div className="flex items-center justify-between gap-3">
        <p className="text-[10px] text-muted-foreground">
          {t("install.profileRule")}
        </p>
        <Button size="sm" disabled={selected.size === 0} onClick={onStart}>
          {t("install.installAndEnable")}
        </Button>
      </div>
    </div>
  )
}

function InstallMetric({ label, value }: { label: string; value: string }) {
  return (
    <div>
      <p className="text-[10px] text-muted-foreground">{label}</p>
      <p className="mt-1 font-medium">{value}</p>
    </div>
  )
}

function InstallBusy({ label }: { label: string }) {
  return (
    <div className="flex min-h-28 items-center justify-center gap-2 text-sm text-muted-foreground">
      <Loader2 className="size-4 animate-spin" aria-hidden="true" />
      {label}
    </div>
  )
}

function ResultState({
  icon,
  title,
  detail,
  children,
}: {
  icon: React.ReactNode
  title: string
  detail?: string | null
  children: React.ReactNode
}) {
  return (
    <div className="space-y-3 py-3">
      <div className="flex items-center gap-2 text-sm font-medium">
        {icon}
        {title}
      </div>
      {detail ? (
        <p className="break-words border-l-2 border-destructive/40 pl-3 text-xs text-muted-foreground">
          {detail}
        </p>
      ) : null}
      <div className="flex items-center gap-2">{children}</div>
    </div>
  )
}

function capitalize(value: string): string {
  return value.charAt(0).toUpperCase() + value.slice(1)
}
