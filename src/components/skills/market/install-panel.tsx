"use client"

import { useEffect, useRef, useState } from "react"
import {
  AlertTriangle,
  Ban,
  Check,
  ChevronDown,
  ChevronRight,
  Download,
  FileArchive,
  Loader2,
  RefreshCw,
  RotateCcw,
  ShieldCheck,
  X,
} from "lucide-react"
import { useTranslations } from "next-intl"
import { Button } from "@/components/ui/button"
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from "@/components/ui/collapsible"
import { Progress } from "@/components/ui/progress"
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from "@/components/ui/tooltip"
import type { useSkillMarketInstall } from "@/hooks/use-skill-market-install"
import {
  formatSkillBytes,
  installErrorAction,
  type SkillMarketInstallArtifactProgress,
  type SkillMarketInstallPhase,
  type SkillMarketTranslator,
} from "@/lib/skill-market"

function PhaseIcon({ phase }: { phase: SkillMarketInstallPhase }) {
  switch (phase) {
    case "downloading":
      return <Download className="size-3.5" aria-hidden="true" />
    case "verifying":
      return <ShieldCheck className="size-3.5" aria-hidden="true" />
    case "extracting":
      return <FileArchive className="size-3.5" aria-hidden="true" />
    case "activating":
      return <Loader2 className="size-3.5 animate-spin" aria-hidden="true" />
    case "done":
      return <Check className="size-3.5 text-emerald-500" aria-hidden="true" />
    case "failed":
      return <AlertTriangle className="size-3.5 text-destructive" aria-hidden="true" />
    case "canceled":
      return <Ban className="size-3.5 text-muted-foreground" aria-hidden="true" />
    case "pending":
      return <span className="size-3.5 rounded-full border" aria-hidden="true" />
  }
}

function ArtifactRow({
  progress,
}: {
  progress: SkillMarketInstallArtifactProgress
}) {
  const t = useTranslations("SkillMarketV2")
  const [open, setOpen] = useState(false)
  const ratio =
    progress.bytesTotal > 0
      ? Math.min(100, (progress.bytesReceived / progress.bytesTotal) * 100)
      : 0
  return (
    <Collapsible open={open} onOpenChange={setOpen}>
      <div className="flex items-center gap-2 rounded-md border px-2.5 py-2">
        <CollapsibleTrigger asChild>
          <Button
            size="icon-xs"
            variant="ghost"
            className="size-5 shrink-0"
            aria-label={t(
              open ? "install.collapse" : "install.expand",
              { name: progress.displayName }
            )}
          >
            {open ? (
              <ChevronDown className="size-3.5" aria-hidden="true" />
            ) : (
              <ChevronRight className="size-3.5" aria-hidden="true" />
            )}
          </Button>
        </CollapsibleTrigger>
        <PhaseIcon phase={progress.phase} />
        <span className="min-w-0 flex-1 truncate text-xs font-medium">
          {progress.displayName}
        </span>
        <span className="shrink-0 font-mono text-[10px] text-muted-foreground">
          v{progress.version}
        </span>
        <span className="shrink-0 text-[10px] text-muted-foreground">
          {t(`install.phase.${progress.phase}`)}
        </span>
      </div>
      <CollapsibleContent>
        <div className="px-3 py-2">
          {progress.errorCode ? (
            <p className="mb-1.5 break-words text-[11px] text-destructive">
              {t(`install.error.${progress.errorCode}`)}
            </p>
          ) : null}
          <Progress value={ratio} className="h-1.5" />
          <p className="mt-1 text-[10px] text-muted-foreground">
            {formatSkillBytes(progress.bytesReceived)} /{" "}
            {formatSkillBytes(progress.bytesTotal)}
          </p>
        </div>
      </CollapsibleContent>
    </Collapsible>
  )
}

export interface SkillMarketInstallPanelProps {
  controller: ReturnType<typeof useSkillMarketInstall>
  pendingTarget: { name: string; version: string } | null
  onInstalled: (skillId: string, version: string) => void
  onClose: () => void
}

export function SkillMarketInstallPanel(props: SkillMarketInstallPanelProps) {
  const t = useTranslations("SkillMarketV2") as unknown as SkillMarketTranslator
  const { session, start, cancel, retry, reset } = props.controller
  const onInstalledRef = useRef(props.onInstalled)
  onInstalledRef.current = props.onInstalled
  const notifiedRef = useRef(false)

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

  const overallRatio =
    session.overallBytes > 0
      ? Math.min(100, (session.receivedBytes / session.overallBytes) * 100)
      : 0
  const errorAction = session.errorCode
    ? installErrorAction(session.errorCode)
    : null

  const close = () => {
    reset()
    props.onClose()
  }

  return (
    <div className="pointer-events-none fixed inset-x-0 bottom-0 z-50 flex justify-center px-4 pb-4">
      <div
        role="dialog"
        aria-label={t("install.title")}
        className="pointer-events-auto w-full max-w-2xl rounded-lg border bg-background p-4 shadow-2xl"
      >
        <div className="mb-3 flex items-center gap-2">
          <h3 className="min-w-0 flex-1 truncate text-sm font-semibold">
            {t("install.title")}
          </h3>
          {session.status === "done" ||
          session.status === "canceled" ||
          session.status === "failed" ? (
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

        {session.status === "resolving" ? (
          <div className="flex items-center gap-2 text-sm text-muted-foreground">
            <Loader2 className="size-4 animate-spin" aria-hidden="true" />
            <span className="min-w-0 truncate">
              {t("install.resolving")}
              {props.pendingTarget
                ? ` · ${props.pendingTarget.name}@${props.pendingTarget.version}`
                : ""}
            </span>
          </div>
        ) : null}

        {session.status === "confirming" && session.plan ? (
          <div className="space-y-3">
            <p className="text-xs leading-5 text-muted-foreground">
              {t("install.confirmHint", {
                count: session.plan.dependencyCount,
                size: formatSkillBytes(session.plan.totalBytes),
              })}
            </p>
            {session.plan.mandatory ? (
              <p className="rounded-md border border-amber-500/30 bg-amber-500/10 px-2.5 py-1.5 text-xs text-amber-700 dark:text-amber-400">
                {t("install.mandatory")}
              </p>
            ) : null}
            <div className="flex flex-wrap gap-2">
              <Button size="sm" onClick={start}>
                {t("install.start")}
              </Button>
              <Button size="sm" variant="outline" onClick={close}>
                {t("install.cancel")}
              </Button>
            </div>
          </div>
        ) : null}

        {session.status === "running" ||
        session.status === "activating" ? (
          <div className="space-y-3">
            <div>
              <div className="mb-1.5 flex items-center justify-between gap-2 text-[10px] text-muted-foreground">
                <span>{t("install.overall")}</span>
                <span className="font-mono">
                  {formatSkillBytes(session.receivedBytes)} /{" "}
                  {formatSkillBytes(session.overallBytes)}
                </span>
              </div>
              <Progress value={overallRatio} />
            </div>
            {session.refreshingTicket ? (
              <p className="flex items-center gap-1.5 text-[11px] text-muted-foreground">
                <RefreshCw className="size-3 animate-spin" aria-hidden="true" />
                {t("install.refreshInBackground")}
              </p>
            ) : null}
            {session.ticketRefreshCount > 0 &&
            !session.refreshingTicket ? (
              <p className="text-[11px] text-muted-foreground">
                {t("install.ticketRefreshed", {
                  count: session.ticketRefreshCount,
                })}
              </p>
            ) : null}
            <div className="max-h-64 space-y-1.5 overflow-y-auto">
              {session.items.map((progress) => (
                <ArtifactRow key={progress.artifactId} progress={progress} />
              ))}
            </div>
            <div className="flex items-center gap-2">
              <Button
                size="sm"
                variant="outline"
                disabled={session.status === "activating"}
                title={
                  session.status === "activating"
                    ? t("install.cancelHint")
                    : undefined
                }
                onClick={cancel}
              >
                {t("install.cancel")}
              </Button>
              {session.status === "activating" ? (
                <span className="text-[10px] text-muted-foreground">
                  {t("install.cancelHint")}
                </span>
              ) : null}
            </div>
          </div>
        ) : null}

        {session.status === "done" ? (
          <div className="space-y-3">
            <p className="flex items-center gap-2 text-sm">
              <Check className="size-4 text-emerald-500" aria-hidden="true" />
              {t("install.done")}
            </p>
            <div className="flex gap-2">
              <Button size="sm" onClick={close}>
                {t("install.doneClose")}
              </Button>
            </div>
          </div>
        ) : null}

        {session.status === "failed" ? (
          <div className="space-y-3">
            <p className="flex items-start gap-2 text-sm text-destructive">
              <AlertTriangle
                className="mt-0.5 size-4 shrink-0"
                aria-hidden="true"
              />
              <span className="min-w-0 break-words">
                {session.errorCode
                  ? t(`install.error.${session.errorCode}`)
                  : t("install.failed")}
              </span>
            </p>
            {session.errorMessage ? (
              <p className="break-words rounded-md bg-muted/40 px-2.5 py-1.5 font-mono text-[10px] text-muted-foreground">
                {session.errorMessage}
              </p>
            ) : null}
            <div className="flex flex-wrap gap-2">
              <Button size="sm" onClick={retry}>
                <RotateCcw className="size-3.5" aria-hidden="true" />
                {t("install.actionRetry")}
              </Button>
              {errorAction && errorAction !== "retry" ? (
                <Tooltip>
                  <TooltipTrigger asChild>
                    <Button
                      size="sm"
                      variant="outline"
                      disabled
                      title={t("install.externalActionHint")}
                    >
                      {t(`install.action${capitalize(errorAction)}`)}
                    </Button>
                  </TooltipTrigger>
                  <TooltipContent>
                    {t("install.externalActionHint")}
                  </TooltipContent>
                </Tooltip>
              ) : null}
            </div>
          </div>
        ) : null}

        {session.status === "canceled" ? (
          <div className="space-y-3">
            <p className="flex items-center gap-2 text-sm text-muted-foreground">
              <Ban className="size-4" aria-hidden="true" />
              {t("install.canceled")}
            </p>
            <div className="flex gap-2">
              <Button size="sm" onClick={retry}>
                <RotateCcw className="size-3.5" aria-hidden="true" />
                {t("install.actionRetry")}
              </Button>
              <Button size="sm" variant="outline" onClick={close}>
                {t("install.close")}
              </Button>
            </div>
          </div>
        ) : null}
      </div>
    </div>
  )
}

function capitalize(value: string): string {
  return value.charAt(0).toUpperCase() + value.slice(1)
}
