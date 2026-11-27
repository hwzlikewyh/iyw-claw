"use client"

import { useCallback } from "react"
import { useTranslations } from "next-intl"

import { useAcpActions } from "@/contexts/acp-connections-context"
import { useConnection } from "@/hooks/use-connection"
import { formatAgentRuntimeError } from "@/lib/agent-runtime-error"
import { ModelNetworkStatus } from "./model-network-status"
import { SessionFailureBanner } from "./session-failure-banner"

function useRuntimeErrorMessages() {
  const t = useTranslations("Folder.chat.acpConnections.backendErrors")
  return {
    insufficientBalance: t("insufficientBalance"),
    authenticationFailed: t("authenticationFailed"),
    permissionDenied: t("permissionDenied"),
    rateLimited: t("rateLimited"),
    quotaExceeded: t("quotaExceeded"),
    modelUnavailable: t("modelUnavailable"),
    requestTimeout: t("requestTimeout"),
    networkError: t("networkError"),
    serviceUnavailable: t("serviceUnavailable"),
    requestFailed: t("requestFailed"),
    workspaceSwitching: t("workspaceSwitching"),
    contextTooLong: t("contextTooLong"),
    requestTooLarge: t("requestTooLarge"),
    contentRejected: t("contentRejected"),
    modelOverloaded: t("modelOverloaded"),
    sessionUnavailable: t("sessionUnavailable"),
    toolProtocolError: t("toolProtocolError"),
    outputLimitReached: t("outputLimitReached"),
    modelStreamInterrupted: t("modelStreamInterrupted"),
    invalidRequest: t("invalidRequest"),
  }
}

export function ConversationRequestStatus({
  contextKey,
}: {
  contextKey: string
}) {
  const { sessionFailures, error } = useConnection(contextKey)
  const { dismissSessionFailures } = useAcpActions()
  const messages = useRuntimeErrorMessages()
  const onDismiss = useCallback(
    (ids: string[]) => dismissSessionFailures(contextKey, ids),
    [contextKey, dismissSessionFailures]
  )
  const failures = sessionFailures.map((failure) => ({
    ...failure,
    title:
      formatAgentRuntimeError(failure.details ?? failure.title, messages) ??
      (failure.id === "prompt-error"
        ? formatAgentRuntimeError(
            `API Error: ${failure.details ?? ""}`,
            messages
          )
        : null) ??
      failure.title,
  }))
  // 已记录的失败统一由可关闭提示呈现；下一轮仍展示实时网络重试。
  const hasRecordedError = failures.some(
    (failure) =>
      failure.severity !== "warning" &&
      (!failure.resolved || error?.startsWith(failure.title))
  )

  return (
    <>
      <SessionFailureBanner failures={failures} onDismiss={onDismiss} />
      {!hasRecordedError && <ModelNetworkStatus contextKey={contextKey} />}
    </>
  )
}
