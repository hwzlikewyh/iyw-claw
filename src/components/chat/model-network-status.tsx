"use client"

import { AlertTriangle, Loader2, WifiOff } from "lucide-react"
import { useTranslations } from "next-intl"

import { useConnection } from "@/hooks/use-connection"
import { useSessionActivity } from "@/hooks/use-session-activity"
import { classifyAgentRuntimeError } from "@/lib/agent-runtime-error"
import { cn } from "@/lib/utils"

const SEVERE_RETRY_ATTEMPTS = 2
const NETWORK_ERROR_KINDS = new Set([
  "networkError",
  "requestTimeout",
  "serviceUnavailable",
])
const NETWORK_ERROR_KEYS = [
  "backendErrors.networkError",
  "backendErrors.requestTimeout",
  "backendErrors.serviceUnavailable",
  "backendErrors.modelStreamInterrupted",
] as const

function isModelNetworkError(
  error: string | null,
  messages: string[]
): boolean {
  if (!error) return false
  if (/max_output_tokens|max_tokens|context_length_exceeded/i.test(error)) {
    return false
  }
  const kind = classifyAgentRuntimeError(`API Error: ${error}`)
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
  const attempt = retry?.retry_attempt ?? 1
  const networkError =
    !loadError &&
    isModelNetworkError(
      error,
      NETWORK_ERROR_KEYS.map((key) => t(key))
    )

  if (!retrying && !networkError) return null

  const severe = attempt >= SEVERE_RETRY_ATTEMPTS
  const message = networkError
    ? t("modelNetwork.disconnected")
    : t(severe ? "modelNetwork.retryingSevere" : "modelNetwork.retrying")

  return (
    <ModelNetworkBanner
      message={message}
      networkError={networkError}
      severe={severe}
      attempts={
        retrying && !networkError
          ? t("modelNetwork.attempts", { count: attempt })
          : null
      }
    />
  )
}

function ModelNetworkBanner({
  message,
  networkError,
  severe,
  attempts,
}: {
  message: string
  networkError: boolean
  severe: boolean
  attempts: string | null
}) {
  const Icon = networkError ? WifiOff : severe ? AlertTriangle : Loader2
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
    </div>
  )
}
