"use client"

import { useCallback, useEffect, useState } from "react"
import {
  connectorTaskActive,
  listConnectorInstallTasks,
  type ConnectorInstallTask,
} from "@/lib/connector-install-tasks"
import { extractAppCommandError } from "@/lib/app-error"
import { onTransportChange } from "@/lib/transport"

const ACTIVE_POLL_MS = 1000
const IDLE_POLL_MS = 10000

export function useConnectorInstallTasks() {
  const [tasks, setTasks] = useState<ConnectorInstallTask[]>([])
  const [error, setError] = useState("")
  const [revision, setRevision] = useState(0)
  const [loaded, setLoaded] = useState(false)
  const [scope, setScope] = useState(0)
  const refresh = useCallback(() => setRevision((value) => value + 1), [])
  useEffect(
    () =>
      onTransportChange(() => {
        setTasks([])
        setError("")
        setLoaded(false)
        setScope((value) => value + 1)
        refresh()
      }),
    [refresh]
  )
  useEffect(() => {
    let active = true
    let timer: ReturnType<typeof setTimeout>
    const load = async () => {
      let running = false
      try {
        const values = await listConnectorInstallTasks()
        if (!active) return
        setTasks(values)
        setLoaded(true)
        setError("")
        running = values.some(connectorTaskActive)
      } catch (cause) {
        if (active) {
          setError(extractAppCommandError(cause)?.message || String(cause))
          running = true
        }
      }
      if (active)
        timer = setTimeout(load, running ? ACTIVE_POLL_MS : IDLE_POLL_MS)
    }
    void load()
    return () => {
      active = false
      clearTimeout(timer)
    }
  }, [revision])
  return { tasks, error, refresh, loaded, scope }
}
