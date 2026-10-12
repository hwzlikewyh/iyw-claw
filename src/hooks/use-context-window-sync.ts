"use client"

import { useEffect, useRef, useState, type RefObject } from "react"
import type { UseConnectionReturn } from "@/hooks/use-connection"
import { CONTEXT_WINDOW_CONFIG_ID } from "@/lib/model-context-window"
import type { SessionConfigOptionInfo } from "@/lib/types"

interface ContextWindowSyncParams {
  connection: UseConnectionReturn
  desired?: SessionConfigOptionInfo
  ready: boolean
  values: Record<string, string>
  conversationId?: number
  onError: (value: string, error: unknown) => void
}

interface ContextWindowRequest extends ContextWindowSyncParams {
  live: SessionConfigOptionInfo
  target: string
  latest: RefObject<{ model: string; target?: string; active: boolean }>
}

function contextPolicyMatches(
  request: Pick<ContextWindowRequest, "live" | "desired" | "values" | "target">
) {
  const automatic = request.live.kind.options.find(
    (option) => option.value === "auto"
  )
  const desiredAuto = request.desired?.kind.options.find(
    (option) => option.value === "auto"
  )
  return (
    request.live.kind.current_value === request.target &&
    automatic?.description === request.values.model &&
    (request.target !== "auto" || automatic?.name === desiredAuto?.name)
  )
}

async function recoverContextWindow(
  request: ContextWindowRequest,
  error: unknown
) {
  const { connection, values, target, live, latest, conversationId, onError } =
    request
  if (
    !latest.current.active ||
    latest.current.model !== values.model ||
    latest.current.target !== target
  )
    return
  onError(live.kind.current_value, error)
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
          [CONTEXT_WINDOW_CONFIG_ID]: live.kind.current_value,
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
      [CONTEXT_WINDOW_CONFIG_ID]: target,
    })
    .catch((error: unknown) => recoverContextWindow(request, error))
}

export function useContextWindowSync(params: ContextWindowSyncParams) {
  const pending = useRef(false)
  const latest = useRef<{ model: string; target?: string; active: boolean }>({
    model: params.values.model,
    active: true,
  })
  const [revision, setRevision] = useState(0)
  const live = params.connection.configOptions?.find(
    (option) => option.id === CONTEXT_WINDOW_CONFIG_ID
  )
  const target =
    params.desired?.kind.current_value ?? (live ? "auto" : undefined)
  const blocked = Boolean(
    target && live && !contextPolicyMatches({ ...params, live, target })
  )
  useEffect(() => {
    latest.current.active = true
    return () => {
      latest.current.active = false
    }
  }, [])
  useEffect(() => {
    latest.current = { model: params.values.model, target, active: true }
    if (pending.current || !live || !target) return
    const change = applyContextWindow({ ...params, live, target, latest })
    if (!change) return
    pending.current = true
    void change.finally(() => {
      pending.current = false
      if (latest.current.active) setRevision((value) => value + 1)
    })
  }, [params, live, target, revision])
  return blocked
}
