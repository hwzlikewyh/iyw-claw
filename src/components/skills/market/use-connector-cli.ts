"use client"

import { useCallback, useEffect, useRef, useState } from "react"
import { getTransport, type Transport } from "@/lib/transport"
import {
  beginConnectorCLI,
  cancelConnectorCLI,
  checkConnector,
  type ConnectorCheck,
} from "@/lib/connector-auth"
import { extractAppCommandError } from "@/lib/app-error"

const message = (cause: unknown) =>
  extractAppCommandError(cause)?.message || String(cause)

function useLifetime(terminal: string | null, transport: Transport) {
  const lease = useRef({
    alive: true,
    terminal,
    running: false,
    restarting: false,
  })
  useEffect(() => {
    const current = lease.current
    current.alive = true
    current.terminal = terminal
    return () => {
      current.alive = false
      current.terminal = null
      if (terminal) void cancelConnectorCLI(terminal, transport).catch(() => {})
    }
  }, [terminal, transport])
  return lease
}

function useVerification({
  id,
  terminal,
  transport,
  onChecked,
  lease,
}: {
  id: string
  terminal: string | null
  transport: Transport
  onChecked: (value: ConnectorCheck) => void
  lease: ReturnType<typeof useLifetime>
}) {
  const [checking, setChecking] = useState(false)
  const [view, setView] = useState<ConnectorCheck | null>(null)
  const [error, setError] = useState("")
  const verify = useCallback(async () => {
    if (
      !lease.current.alive ||
      lease.current.running ||
      lease.current.restarting
    )
      return
    lease.current.running = true
    setChecking(true)
    setError("")
    try {
      if (terminal) await cancelConnectorCLI(terminal, transport)
      const value = await checkConnector(id, transport)
      if (lease.current.alive) {
        setView(value)
        onChecked(value)
      }
    } catch (cause) {
      if (lease.current.alive) setError(message(cause))
    } finally {
      lease.current.running = false
      if (lease.current.alive) setChecking(false)
    }
  }, [id, terminal, onChecked, transport, lease])
  const clear = () => {
    setView(null)
    setError("")
  }
  const fail = (cause: unknown) => setError(message(cause))
  return { checking, view, error, verify, clear, fail }
}

export function useConnectorCLI(
  id: string,
  onChecked: (value: ConnectorCheck) => void
) {
  const [transport] = useState(getTransport)
  const [terminal, setTerminal] = useState<string | null>(null)
  const lease = useLifetime(terminal, transport)
  const state = useVerification({ id, terminal, transport, onChecked, lease })
  const spawn = useCallback(
    async (terminalId: string) => {
      lease.current.restarting = false
      await beginConnectorCLI(id, terminalId, transport)
      if (!lease.current.alive || lease.current.terminal !== terminalId)
        await cancelConnectorCLI(terminalId, transport)
    },
    [id, transport, lease]
  )
  const restart = async () => {
    if (!terminal) return
    lease.current.restarting = true
    try {
      await cancelConnectorCLI(terminal, transport)
      setTerminal(null)
      state.clear()
    } catch (cause) {
      state.fail(cause)
    } finally {
      lease.current.restarting = false
    }
  }
  return {
    ...state,
    terminal,
    spawn,
    restart,
    start: () => setTerminal(`mcp-cli-${crypto.randomUUID()}`),
  }
}
