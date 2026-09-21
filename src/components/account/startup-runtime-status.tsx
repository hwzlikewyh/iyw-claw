"use client"

import {
  Check,
  Copy,
  FolderOpen,
  Loader2,
  Minus,
  RotateCw,
  X,
} from "lucide-react"
import { useTranslations } from "next-intl"
import { useState } from "react"

import { Button } from "@/components/ui/button"
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from "@/components/ui/tooltip"
import { openLogsDir } from "@/lib/api"
import { isLocalDesktop } from "@/lib/platform"
import type { BootstrapComponentStatus, BootstrapInitEvent } from "@/lib/types"
import { copyTextToClipboard } from "@/lib/utils"

const COMPONENTS: Record<string, string> = {
  node: "Node.js",
  git: "Git",
  uv: "uv",
  chromix: "Chromix",
  "agent-browser": "agent-browser",
  officecli: "OfficeCLI",
  "agent-reach": "Agent Reach",
  "open-computer-use": "Open Computer Use",
  "memory-embedding-bge-small-zh-v1.5": "BGE Small",
}

export function updateRuntimeComponent(
  current: BootstrapComponentStatus[],
  event: BootstrapInitEvent
) {
  const id = event.component!
  const previous = current.find((item) => item.componentId === id)
  const component: BootstrapComponentStatus = {
    componentId: id,
    componentKind: previous?.componentKind ?? "runtime_tool",
    version: previous?.version ?? "",
    installed: event.phase === "ready" || Boolean(previous?.installed),
    active: event.phase === "ready" || Boolean(previous?.active),
    phase: event.phase,
    lastError:
      event.phase === "blocked" || event.phase === "degraded"
        ? event.message
        : null,
  }
  return [...current.filter((item) => item.componentId !== id), component]
}

export function StartupRuntimeStatus({
  components,
}: {
  components: BootstrapComponentStatus[]
}) {
  return (
    <ul className="grid max-h-64 gap-2 overflow-y-auto text-sm">
      {Object.entries(COMPONENTS)
        .filter(
          ([id]) =>
            (!isLocalDesktop() && ["node", "git", "uv"].includes(id)) ||
            components.some((component) => component.componentId === id)
        )
        .map(([id, label]) => {
          const component = components.find((item) => item.componentId === id)
          const ready = component?.installed && component.active
          const failed = Boolean(component?.lastError)
          const pending = !component || component.phase === "not_started"
          const Icon = ready ? Check : failed ? X : pending ? Minus : Loader2
          return (
            <li key={id} className="flex items-center justify-between gap-3">
              <span>{label}</span>
              <span className="flex min-w-0 items-center gap-2 text-muted-foreground">
                <span className="truncate text-xs">{component?.version}</span>
                <Icon
                  aria-label={component?.phase ?? "not_started"}
                  className={`size-4 shrink-0 ${ready ? "text-green-600" : failed ? "text-destructive" : pending ? "" : "animate-spin"}`}
                />
              </span>
            </li>
          )
        })}
    </ul>
  )
}

export function StartupFailureActions({ detail }: { detail: string }) {
  const t = useTranslations("StartupCodex")
  const logs = useTranslations("LogsSettings")
  const [copied, setCopied] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const copy = async () => {
    setError(null)
    const success = await copyTextToClipboard(detail)
    setCopied(success)
    if (!success) setError(t("copyFailed"))
  }
  const openLogs = async () => {
    setError(null)
    try {
      await openLogsDir()
    } catch (cause) {
      console.warn("[StartupCodexGate] Failed to open logs:", cause)
      setError(logs("openFolderFailed"))
    }
  }
  return (
    <div className="grid gap-2">
      <div className="flex justify-center gap-2">
        <Tooltip>
          <TooltipTrigger asChild>
            <Button
              variant="outline"
              size="icon"
              aria-label={t("copyError")}
              onClick={() => void copy()}
            >
              {copied ? (
                <Check className="size-4" />
              ) : (
                <Copy className="size-4" />
              )}
            </Button>
          </TooltipTrigger>
          <TooltipContent>{t("copyError")}</TooltipContent>
        </Tooltip>
        {isLocalDesktop() ? (
          <Tooltip>
            <TooltipTrigger asChild>
              <Button
                variant="outline"
                size="icon"
                aria-label={logs("logsPathLabel")}
                onClick={() => void openLogs()}
              >
                <FolderOpen className="size-4" />
              </Button>
            </TooltipTrigger>
            <TooltipContent>{logs("logsPathLabel")}</TooltipContent>
          </Tooltip>
        ) : null}
      </div>
      {error ? (
        <p role="alert" className="text-xs text-destructive">
          {error}
        </p>
      ) : null}
    </div>
  )
}

export function StartupFailureDetails({
  step,
  detail,
  onRetry,
}: {
  step: string
  detail: string
  onRetry: () => void
}) {
  const t = useTranslations("StartupCodex")
  return (
    <div className="grid gap-4">
      <div className="grid gap-1.5">
        <p className="text-xs font-medium text-muted-foreground">
          {t("errorStep", { step })}
        </p>
        <pre className="max-h-40 overflow-auto rounded-md bg-muted p-2 text-left text-xs break-all whitespace-pre-wrap">
          {detail}
        </pre>
      </div>
      <Button size="sm" className="mx-auto" onClick={onRetry}>
        <RotateCw className="size-4" />
        {t("retry")}
      </Button>
      <StartupFailureActions key={detail} detail={`${step}\n${detail}`} />
    </div>
  )
}
