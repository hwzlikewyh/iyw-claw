"use client"

import { useEffect, useRef, useState, type RefObject } from "react"
import type { UseConnectionReturn } from "@/hooks/use-connection"
import { isContextBudgetConfigId } from "@/lib/model-context-window"
import { isModelConfigOption } from "@/lib/model-display-name"
import {
  SendPreflightTimeout,
  waitForSendPreflight,
} from "@/lib/send-preflight"
import type { SessionConfigOptionInfo } from "@/lib/types"

interface ContextWindowSyncParams {
  connection: UseConnectionReturn
  desired: SessionConfigOptionInfo[]
  ready: boolean
  values: Record<string, string>
  conversationId?: number
  onError: (values: Record<string, string>, error: unknown) => void
}

interface ContextWindowRequest extends ContextWindowSyncParams {
  live: SessionConfigOptionInfo[]
  target: Record<string, string>
  signature: string
  latest: RefObject<{
    signature: string
    connectionId: string | null
    active: boolean
  }>
}

function contextPolicyMatches(
  request: Pick<ContextWindowRequest, "live" | "values" | "target">
) {
  // 目录中的 Auto 额度变化在下一次启动生效，不重启仍然存活的会话。
  return request.live.every(
    (option) =>
      option.kind.current_value === request.target[option.id] &&
      (!request.values.model ||
        option.kind.options.find((value) => value.value === "auto")
          ?.description === request.values.model)
  )
}

async function recoverContextWindow(
  request: ContextWindowRequest,
  error: unknown
) {
  const {
    connection,
    values,
    signature,
    live,
    latest,
    conversationId,
    onError,
  } = request
  const current = latest.current
  if (
    !current.active ||
    current.signature !== signature ||
    (current.connectionId && current.connectionId !== connection.connectionId)
  )
    return
  const previous = Object.fromEntries(
    live.map((option) => [option.id, option.kind.current_value])
  )
  onError(previous, error)
  if (!connection.agentType || error instanceof SendPreflightTimeout) return
  await connection
    .connect(
      connection.agentType,
      connection.connectedWorkingDir ?? undefined,
      connection.sessionId ?? undefined,
      conversationId,
      {
        forceHostRestart: true,
        preferredConfigValues: {
          ...values,
          ...previous,
        },
      }
    )
    .catch((recoveryError: unknown) => {
      console.error(
        "[ContextWindow] restore previous session configuration failed",
        {
          agentType: connection.agentType,
          model: values.model,
          error: recoveryError,
        }
      )
    })
}

function applyContextWindow(
  request: ContextWindowRequest
): Promise<unknown> | null {
  const { connection, ready, values, target, conversationId } = request
  if (
    !ready ||
    connection.status !== "connected" ||
    connection.isViewer ||
    connection.isDelegationChild
  )
    return null
  const model = connection.configOptions?.find(isModelConfigOption)
  if (
    contextPolicyMatches(request) ||
    (model && values.model && model.kind.current_value !== values.model)
  )
    return null
  return waitForSendPreflight(
    connection.reapplyConfig(true, conversationId, {
      ...values,
      ...target,
    }),
    "context_timeout"
  ).catch(async (error: unknown) => {
    await recoverContextWindow(request, error)
    return false
  })
}

export function useContextWindowSync(params: ContextWindowSyncParams) {
  const pending = useRef<{ connectionId: string | null } | null>(null)
  const deferred = useRef<string | null>(null)
  const latest = useRef<ContextWindowRequest["latest"]["current"]>({
    signature: "",
    connectionId: null,
    active: true,
  })
  const [revision, setRevision] = useState(0)
  const live = (params.connection.configOptions ?? []).filter((option) =>
    isContextBudgetConfigId(option.id)
  )
  const target = Object.fromEntries(
    live.map((option) => [
      option.id,
      params.values[option.id] ??
        params.desired.find((desired) => desired.id === option.id)?.kind
          .current_value ??
        option.kind.current_value,
    ])
  )
  for (const option of params.desired) {
    target[option.id] ??= params.values[option.id] ?? option.kind.current_value
  }
  const connectionId = params.connection.connectionId
  const signature = JSON.stringify([
    params.conversationId,
    params.values.model,
    target,
  ])
  const attemptKey = JSON.stringify([connectionId, signature])
  const blocked =
    !params.connection.isViewer &&
    !params.connection.isDelegationChild &&
    live.length > 0 &&
    !contextPolicyMatches({ ...params, live, target })
  useEffect(() => {
    latest.current.active = true
    return () => {
      latest.current.active = false
    }
  }, [])
  useEffect(() => {
    latest.current = { signature, connectionId, active: true }
    if (!blocked || deferred.current !== attemptKey) deferred.current = null
    if (
      !blocked ||
      pending.current?.connectionId === connectionId ||
      deferred.current === attemptKey
    )
      return
    const change = applyContextWindow({
      ...params,
      live,
      target,
      signature,
      latest,
    })
    if (!change) return
    const attempt = { connectionId }
    pending.current = attempt
    void change
      .then((applied) => {
        if (applied === false && pending.current === attempt)
          deferred.current = attemptKey
      })
      .catch((error: unknown) => {
        if (pending.current === attempt) deferred.current = attemptKey
        console.error("[ContextWindow] configuration recovery failed", error)
      })
      .finally(() => {
        if (pending.current !== attempt) return
        pending.current = null
        if (latest.current.active) setRevision((value) => value + 1)
      })
  }, [
    params,
    live,
    target,
    signature,
    attemptKey,
    revision,
    blocked,
    connectionId,
  ])
  return blocked
}
