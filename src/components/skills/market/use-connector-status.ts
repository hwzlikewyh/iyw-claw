"use client"

import { useEffect, useRef, useState } from "react"
import {
  checkConnector,
  logoutConnectorCLI,
  type ConnectorCheck,
} from "@/lib/connector-auth"
import { extractAppCommandError } from "@/lib/app-error"

const READY_DISPLAY_MS = 60_000

function useLifetime() {
  const lease = useRef({ alive: true, running: false })
  useEffect(() => {
    const current = lease.current
    current.alive = true
    return () => {
      current.alive = false
    }
  }, [])
  return lease
}

function useExpiry(
  view: ConnectorCheck | null,
  clear: (value: ConnectorCheck | null) => void
) {
  useEffect(() => {
    if (view?.state !== "ready") return
    const timer = setTimeout(() => clear(null), READY_DISPLAY_MS)
    return () => clearTimeout(timer)
  }, [view, clear])
}

export function useConnectorStatus(id: string) {
  const [view, setView] = useState<ConnectorCheck | null>(null)
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState("")
  const [dialog, setDialog] = useState(false)
  const lease = useLifetime()
  useExpiry(view, setView)
  const inspect = async (logout = false) => {
    if (lease.current.running) return
    lease.current.running = true
    setBusy(true)
    setError("")
    setView(null)
    try {
      if (logout) await logoutConnectorCLI(id)
      const value = await checkConnector(id)
      if (lease.current.alive) setView(value)
    } catch (cause) {
      if (lease.current.alive)
        setError(extractAppCommandError(cause)?.message || String(cause))
    } finally {
      lease.current.running = false
      if (lease.current.alive) setBusy(false)
    }
  }
  const open = () => {
    setView(null)
    setError("")
    setDialog(true)
  }
  return { view, busy, error, dialog, setDialog, setView, inspect, open }
}
