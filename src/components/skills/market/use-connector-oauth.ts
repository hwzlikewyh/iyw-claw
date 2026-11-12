"use client"

import { useEffect, useRef, useState } from "react"
import { getTransport } from "@/lib/transport"
import { openUrl } from "@/lib/platform"
import { extractAppCommandError } from "@/lib/app-error"

export interface OAuthStatus {
  id: string
  state:
    | "starting"
    | "pending"
    | "completed"
    | "cancelled"
    | "failed"
    | "expired"
  authorization_url: string | null
  error: string | null
}

interface RequestLease {
  key: string
  promise: Promise<OAuthStatus>
  leases: number
}

const message = (cause: unknown) =>
  extractAppCommandError(cause)?.message || String(cause)
const cancel = (id: string) =>
  getTransport().call("mcp_oauth_cancel", { attemptId: id })

function acquire(
  id: string,
  key: string,
  ref: { current: RequestLease | null }
) {
  if (ref.current?.key !== key) {
    ref.current = {
      key,
      leases: 0,
      promise: getTransport().call<OAuthStatus>("mcp_oauth_begin", {
        serverId: id,
      }),
    }
  }
  ref.current.leases++
  return ref.current
}

function observe(
  lease: RequestLease,
  update: (value: OAuthStatus) => void,
  fail: (text: string) => void
) {
  let active = true
  let timer: ReturnType<typeof setTimeout>
  const poll = (value: OAuthStatus) => {
    if (!active) return
    update(value)
    if (!["starting", "pending"].includes(value.state)) return
    timer = setTimeout(() => {
      getTransport()
        .call<OAuthStatus>("mcp_oauth_status", { attemptId: value.id })
        .then(poll)
        .catch((cause) => {
          if (active) fail(message(cause))
        })
    }, 1000)
  }
  lease.promise.then(poll).catch((cause) => {
    if (active) fail(message(cause))
  })
  return () => {
    active = false
    clearTimeout(timer)
    lease.leases--
    queueMicrotask(() => {
      if (!lease.leases)
        void lease.promise.then((value) => cancel(value.id)).catch(() => {})
    })
  }
}

function useAuthorizationPage(
  status: OAuthStatus | null,
  openedRef: { current: string | null },
  setError: (value: string) => void
) {
  const url = status?.state === "pending" ? status.authorization_url : null
  useEffect(() => {
    if (!url || openedRef.current === url) return
    let active = true
    openedRef.current = url
    void openUrl(url).catch((cause) => {
      if (active) setError(message(cause))
    })
    return () => {
      active = false
    }
  }, [url, openedRef, setError])
}

export function useConnectorOAuth(id: string) {
  const [status, setStatus] = useState<OAuthStatus | null>(null)
  const [error, setError] = useState("")
  const [generation, setGeneration] = useState(0)
  const [restarting, setRestarting] = useState(false)
  const request = useRef<RequestLease | null>(null)
  const openedRef = useRef<string | null>(null)
  useEffect(
    () =>
      observe(acquire(id, `${id}:${generation}`, request), setStatus, setError),
    [id, generation]
  )
  useAuthorizationPage(status, openedRef, setError)
  const retry = async () => {
    if (restarting) return
    setRestarting(true)
    try {
      const value = await request.current?.promise.catch(() => null)
      if (value) await cancel(value.id)
      openedRef.current = null
      setError("")
      setStatus(null)
      setGeneration((value) => value + 1)
    } catch (cause) {
      setError(message(cause))
    } finally {
      setRestarting(false)
    }
  }
  const open = () => {
    if (status?.state === "pending" && status.authorization_url)
      void openUrl(status.authorization_url).catch((cause) =>
        setError(message(cause))
      )
  }
  return { status, error, restarting, retry, open }
}
