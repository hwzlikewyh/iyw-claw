"use client"

import { useEffect, useRef, useState, type RefObject } from "react"
import type { UseConnectionReturn } from "@/hooks/use-connection"
import { isContextBudgetConfigId } from "@/lib/model-context-window"
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
  latest: RefObject<{ signature: string; active: boolean }>
}

function contextPolicyMatches(
  request: Pick<ContextWindowRequest, "live" | "values" | "target">
) {
  // 目录中的 Auto 额度变化在下一次启动生效，不重启仍然存活的会话。
  return request.live.every(
    (option) =>
      option.kind.current_value === request.target[option.id] &&
      option.kind.options.find((value) => value.value === "auto")
        ?.description === request.values.model
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
  if (!latest.current.active || latest.current.signature !== signature) return
  const previous = Object.fromEntries(
    live.map((option) => [option.id, option.kind.current_value])
  )
  onError(previous, error)
  if (!connection.agentType) return
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
  if (
    contextPolicyMatches(request) ||
    connection.configOptions?.find((option) => option.id === "model")?.kind
      .current_value !== values.model
  )
    return null
  return connection
    .reapplyConfig(true, conversationId, {
      ...values,
      ...target,
    })
    .catch((error: unknown) => recoverContextWindow(request, error))
}

export function useContextWindowSync(params: ContextWindowSyncParams) {
  const pending = useRef(false)
  const latest = useRef<{ signature: string; active: boolean }>({
    signature: "",
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
  const signature = JSON.stringify({ model: params.values.model, ...target })
  const blocked =
    live.length > 0 && !contextPolicyMatches({ ...params, live, target })
  useEffect(() => {
    latest.current.active = true
    return () => {
      latest.current.active = false
    }
  }, [])
  useEffect(() => {
    latest.current = { signature, active: true }
    if (pending.current || live.length === 0) return
    const change = applyContextWindow({
      ...params,
      live,
      target,
      signature,
      latest,
    })
    if (!change) return
    pending.current = true
    void change.finally(() => {
      pending.current = false
      if (latest.current.active) setRevision((value) => value + 1)
    })
  }, [params, live, target, signature, revision])
  return blocked
}
