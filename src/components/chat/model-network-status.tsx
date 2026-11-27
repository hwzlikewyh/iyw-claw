"use client"

import { useEffect, useMemo, useState, type ReactNode } from "react"
import { AlertTriangle, ChevronDown, Loader2 } from "lucide-react"
import { useTranslations } from "next-intl"

import { useConnection } from "@/hooks/use-connection"
import { useSessionActivity } from "@/hooks/use-session-activity"
import { useVisibleNow } from "@/hooks/use-visible-now"
import type { ObservedSessionActivity } from "@/lib/session-activity"
import { classifyAgentRuntimeError } from "@/lib/agent-runtime-error"
import {
  clearModelNetworkDiagnosis,
  getModelNetworkDiagnosis,
  type ModelNetworkDiagnosis,
  type NetworkProbeStatus,
} from "@/lib/model-network-diagnostics"
import { cn } from "@/lib/utils"

const MILLISECONDS_PER_SECOND = 1_000
const SEVERE_RETRY_ATTEMPTS = 2
const NETWORK_ERROR_KINDS = new Set([
  "networkError",
  "requestTimeout",
  "serviceUnavailable",
  "modelOverloaded",
  "modelStreamInterrupted",
])
const NETWORK_ERROR_KEYS = [
  "backendErrors.networkError",
  "backendErrors.requestTimeout",
  "backendErrors.serviceUnavailable",
  "backendErrors.modelStreamInterrupted",
] as const
const NETWORK_RETRY_REASONS = new Set([
  "networkError",
  "serverError",
  "streamDisconnected",
])
const NON_NETWORK_FAILURE_RE =
  /insufficient_points|insufficient[ _-]+(?:balance|quota|credit|funds)|(?:balance|quota|credit)[ _-]+(?:insufficient|exhausted)|credit balance (?:is )?too low|payment[ _-]+required|billing|余额不足|额度不足|配额不足|欠费/i

function isModelNetworkError(
  error: string | null,
  messages: string[]
): boolean {
  if (!error) return false
  if (
    /max_output_tokens|max_tokens|context_length_exceeded/i.test(error) ||
    NON_NETWORK_FAILURE_RE.test(error)
  ) {
    return false
  }
  const kind =
    classifyAgentRuntimeError(error) ??
    classifyAgentRuntimeError(`API Error: ${error}`)
  if (kind && kind !== "requestFailed") return NETWORK_ERROR_KINDS.has(kind)
  return (
    messages.some((message) => error.startsWith(message)) ||
    /^(?:Internal error:\s*["']?)?stream disconnected before completion:/i.test(
      error
    )
  )
}

export function ModelNetworkStatus({ contextKey }: { contextKey: string }) {
  const t = useTranslations("Folder.chat.acpConnections")
  const { error, loadError, status } = useConnection(contextKey)
  const activity = useSessionActivity(contextKey)
  const retry = activity?.snapshot
  const retrying = status === "prompting" && Boolean(retry?.retrying_since)
  const attempt = retry?.retry_attempt
  const networkError =
    !loadError &&
    isModelNetworkError(
      error,
      NETWORK_ERROR_KEYS.map((key) => t(key))
    )
  const gatewayRetry =
    retrying &&
    activity &&
    retry?.retry_at &&
    attempt &&
    retry.retry_max_attempts
      ? {
          activity,
          retryAt: retry.retry_at,
          progress: { attempt, max: retry.retry_max_attempts },
        }
      : null
  const retryReason = retry?.retry_reason
  const networkIssue =
    networkError ||
    Boolean(gatewayRetry) ||
    (retrying && (!retryReason || NETWORK_RETRY_REASONS.has(retryReason)))
  const diagnosisScope = useMemo(
    () => ({ contextKey, networkIssue }),
    [contextKey, networkIssue]
  )

  const [diagnosisState, setDiagnosisState] = useState<{
    scope: typeof diagnosisScope
    diagnosis: ModelNetworkDiagnosis | null
  }>({ scope: diagnosisScope, diagnosis: null })

  useEffect(() => {
    let cancelled = false
    if (!networkIssue) {
      clearModelNetworkDiagnosis(contextKey)
      return
    }

    getModelNetworkDiagnosis(contextKey)
      .then((diagnosis) => {
        if (!cancelled) setDiagnosisState({ scope: diagnosisScope, diagnosis })
      })
      .catch(() => {
        if (!cancelled)
          setDiagnosisState({ scope: diagnosisScope, diagnosis: null })
      })
    return () => {
      cancelled = true
    }
  }, [contextKey, diagnosisScope, networkIssue])

  if (!retrying && !networkError) return null

  const severe = (attempt ?? 0) >= SEVERE_RETRY_ATTEMPTS
  const message = networkError
    ? t("modelNetwork.disconnected")
    : t("modelNetwork.retrying")

  return (
    <ModelNetworkBanner
      message={
        gatewayRetry ? <GatewayRetryMessage {...gatewayRetry} /> : message
      }
      networkError={networkError && !gatewayRetry}
      severe={severe}
      attempts={
        gatewayRetry
          ? (retryReason ?? null)
          : retrying && !networkError && attempt != null && attempt > 0
            ? t("modelNetwork.attempts", { count: attempt })
            : null
      }
      diagnosis={
        networkIssue && diagnosisState.scope === diagnosisScope
          ? diagnosisState.diagnosis
          : null
      }
      diagnosisLoading={networkIssue && diagnosisState.scope !== diagnosisScope}
    />
  )
}

function GatewayRetryMessage({
  activity,
  retryAt,
  progress,
}: {
  activity: ObservedSessionActivity
  retryAt: string
  progress: { attempt: number; max: number }
}) {
  const t = useTranslations("Folder.chat.acpConnections.modelNetwork")
  const now = useVisibleNow(true)
  const elapsed = Math.max(0, now - activity.receivedAt)
  const remaining =
    Date.parse(retryAt) - Date.parse(activity.snapshot.sampled_at) - elapsed
  const seconds = Math.ceil(remaining / MILLISECONDS_PER_SECOND)
  if (seconds <= 0) return t("reconnectNow", progress)
  return (
    <>
      {t("reconnectWait", progress)}
      <span role="timer" aria-live="off" className="ms-2">
        {t("retryIn", { seconds })}
      </span>
    </>
  )
}

function ModelNetworkBanner({
  message,
  networkError,
  severe,
  attempts,
  diagnosis,
  diagnosisLoading,
}: {
  message: ReactNode
  networkError: boolean
  severe: boolean
  attempts: string | null
  diagnosis: ModelNetworkDiagnosis | null
  diagnosisLoading: boolean
}) {
  const t = useTranslations("Folder.chat.acpConnections.modelNetwork")
  const Icon = networkError || severe ? AlertTriangle : Loader2
  return (
    <div
      role={networkError || severe ? "alert" : "status"}
      aria-live="polite"
      className={cn(
        "mb-2 shrink-0 rounded-md border px-3 py-2 text-xs",
        networkError
          ? "border-destructive/20 bg-destructive/5 text-destructive"
          : "border-amber-500/30 bg-amber-500/10 text-amber-700 dark:text-amber-300"
      )}
    >
      <div className="flex items-start gap-2">
        <Icon
          aria-hidden="true"
          className={cn(
            "mt-0.5 size-4 shrink-0",
            !networkError && !severe && "animate-spin"
          )}
        />
        <p className="min-w-0 break-words leading-relaxed">
          {message}
          {attempts && <span className="ms-2 opacity-80">{attempts}</span>}
        </p>
      </div>
      {diagnosisLoading ? <p className="mt-2">{t("checking")}</p> : null}
      {diagnosis ? <NetworkDiagnosisDetails diagnosis={diagnosis} /> : null}
    </div>
  )
}

function NetworkDiagnosisDetails({
  diagnosis,
}: {
  diagnosis: ModelNetworkDiagnosis
}) {
  const t = useTranslations("Folder.chat.acpConnections.modelNetwork")
  return (
    <details className="mt-2 border-t border-current/15 pt-2">
      <summary className="flex cursor-pointer list-none items-center gap-1 font-medium">
        <ChevronDown aria-hidden="true" className="size-3.5" />
        {t("details")}
      </summary>
      <div className="mt-2 space-y-2 ps-5 leading-relaxed">
        <ProbeDetail status={diagnosis.client} />
        {diagnosis.appServer && <ProbeDetail status={diagnosis.appServer} />}
        {diagnosis.server && <ProbeDetail status={diagnosis.server} />}
        <p>
          <span className="font-medium">{t("applicationProxy")}</span>{" "}
          {t(`proxy.${diagnosis.applicationProxy}`)}
        </p>
        <p className="text-muted-foreground">
          {t(summarizeDiagnosis(diagnosis))}
        </p>
      </div>
    </details>
  )
}

function ProbeDetail({ status }: { status: NetworkProbeStatus }) {
  const t = useTranslations("Folder.chat.acpConnections.modelNetwork")
  const value = t(`status.${status.status}`)
  const latency =
    status.elapsedMs === undefined
      ? ""
      : t("latency", { value: status.elapsedMs })
  const failure = status.failure ? t(`failure.${status.failure}`) : ""
  return (
    <p>
      <span className="font-medium">{t(`target.${status.target}`)}</span>{" "}
      {value}
      {latency ? ` (${latency})` : ""}
      {failure ? `: ${failure}` : ""}
      {status.status === "failed" ? ` ${t(`action.${status.target}`)}` : ""}
    </p>
  )
}

function summarizeDiagnosis(diagnosis: ModelNetworkDiagnosis) {
  if (
    diagnosis.client.target === "app_server" &&
    diagnosis.client.status === "failed"
  ) {
    return "summary.clientOrServer" as const
  }
  if (
    diagnosis.appServer?.status === "failed" &&
    diagnosis.client.status !== "failed" &&
    diagnosis.client.status !== "unknown"
  ) {
    return "summary.remoteConnection" as const
  }
  if (
    diagnosis.client.status === "failed" &&
    diagnosis.applicationProxy === "enabled"
  ) {
    return "summary.localNetworkAndProxy" as const
  }
  if (diagnosis.client.status === "failed") {
    return "summary.localNetwork" as const
  }
  if (diagnosis.server?.status === "failed") {
    return "summary.server" as const
  }
  if (diagnosis.client.status === "slow") return "summary.slow" as const
  if (diagnosis.client.status === "unknown") return "summary.unknown" as const
  return "summary.reachable" as const
}
