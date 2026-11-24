"use client"

import { useCallback, useEffect, useRef, useState } from "react"
import { checkConnector, type ConnectorCheck } from "@/lib/connector-auth"
import { extractAppCommandError } from "@/lib/app-error"
import { getTransport, type Transport } from "@/lib/transport"
import type { useConnectorInstall } from "./use-connector-install"

type SetupState = {
  phase: "checking" | "authorizing" | "result"
  view: ConnectorCheck | null
  error: string
}

export function getConnectorSetup(
  state: ReturnType<typeof useConnectorInstall>,
  installed: boolean
) {
  if (state.task?.phase !== "completed") return null
  const autoAuthorize = state.task.id === state.submittedTaskId
  if (!installed && !autoAuthorize) return null
  const option = state.detail?.install_options.find(
    (item) => item.id === (state.task?.optionId ?? state.optionId)
  )
  const mode = (option?.spec.iyw_install as { auth_mode?: string } | undefined)
    ?.auth_mode
  if (mode !== "oauth" && mode !== "cli") return null
  return { taskId: state.task.id, mode, autoAuthorize } as const
}

function useSetupVerification({
  id,
  transport,
  update,
}: {
  id: string
  transport: Transport
  update: (state: SetupState) => void
}) {
  const generation = useRef(0)
  const pending = useRef<Promise<ConnectorCheck> | null>(null)
  useEffect(
    () => () => {
      generation.current++
    },
    []
  )
  return useCallback(
    async (autoLogin = false) => {
      const request = ++generation.current
      update({ phase: "checking", view: null, error: "" })
      try {
        pending.current ??= checkConnector(id, transport).finally(() => {
          pending.current = null
        })
        const view = await pending.current
        if (generation.current !== request) return
        update({
          phase:
            autoLogin && view.state === "authentication_required"
              ? "authorizing"
              : "result",
          view,
          error: "",
        })
      } catch (cause) {
        if (generation.current !== request) return
        update({
          phase: "result",
          view: null,
          error: extractAppCommandError(cause)?.message || String(cause),
        })
      }
    },
    [id, transport, update]
  )
}

export function useConnectorSetup(id: string, autoAuthorize: boolean) {
  const [transport] = useState(getTransport)
  const [state, setState] = useState<SetupState>({
    phase: autoAuthorize ? "authorizing" : "checking",
    view: null,
    error: "",
  })
  const check = useSetupVerification({ id, transport, update: setState })
  useEffect(() => {
    if (!autoAuthorize) void check(true)
  }, [autoAuthorize, check])
  const checked = useCallback((view: ConnectorCheck) => {
    setState({ phase: "result", view, error: "" })
  }, [])
  const verify = useCallback(() => {
    void check()
  }, [check])
  return {
    ...state,
    checked,
    verify,
    authorize: () => setState({ phase: "authorizing", view: null, error: "" }),
  }
}
