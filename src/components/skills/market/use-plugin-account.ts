"use client"

import { useCallback, useEffect, useReducer, useRef, useState } from "react"
import { getTransport } from "@/lib/transport"
import { openUrl } from "@/lib/platform"
import { extractAppCommandError } from "@/lib/app-error"
import {
  operatePluginAccount,
  type PluginPreparationChange,
  type PluginAccountStatus,
} from "@/lib/plugin-preparation"

const POLL_MS = 2000,
  LIFETIME_MS = 300_000,
  STATE_TTL_MS = 60_000
type Input = PluginPreparationChange & { connectorKey: string }
type Operation = "status" | "login" | "logout"
interface State {
  status: PluginAccountStatus | null
  error: string
  busy: boolean
  pending: boolean
  expired: boolean
}
type Update = (value: Partial<State>) => void
const message = (cause: unknown) =>
  extractAppCommandError(cause)?.message || String(cause)

export function usePluginAccount(input: Input, onChanged: () => void) {
  const [transport] = useState(getTransport)
  const [state, update] = useReducer(
    (current: State, patch: Partial<State>) => ({ ...current, ...patch }),
    { status: null, error: "", busy: false, pending: false, expired: false }
  )
  const leaseRef = useAccountLease()
  const callbacks = useRef({ input, onChanged })
  useEffect(() => {
    callbacks.current = { input, onChanged }
  })
  const act = useAccountActions({
    transport,
    lease: leaseRef,
    callbacks,
    update,
  })
  useAccountPolling(state.pending, act, { lease: leaseRef, update })
  useAccountExpiry(state, update)
  const stop = () => {
    leaseRef.current.generation++
    leaseRef.current.url = null
    leaseRef.current.running = false
    update({ busy: false, pending: false, status: null })
  }
  return {
    ...state,
    act,
    stop,
    open: () =>
      state.status?.authorizationUrl
        ? openUrl(state.status.authorizationUrl).catch((cause) => {
            if (leaseRef.current.alive) update({ error: message(cause) })
          })
        : Promise.resolve(),
  }
}

function useAccountLease() {
  const lease = useRef({
    alive: true,
    generation: 0,
    deadline: 0,
    running: false,
    url: null as string | null,
  })
  useEffect(() => {
    const current = lease.current
    current.alive = true
    return () => {
      current.alive = false
      current.generation++
    }
  }, [])
  return lease
}

interface Actions {
  transport: ReturnType<typeof getTransport>
  lease: ReturnType<typeof useAccountLease>
  callbacks: { current: { input: Input; onChanged: () => void } }
  update: Update
}

function useAccountActions(state: Actions) {
  const callbacks = useRef(state)
  useEffect(() => {
    callbacks.current = state
  })
  return useCallback(async (operation: Operation, poll = false) => {
    const s = callbacks.current,
      lease = s.lease.current
    if (lease.running || !lease.alive) return false
    lease.running = true
    const generation = ++lease.generation
    if (!poll) {
      s.update({ busy: true, error: "", status: null, expired: false })
      s.callbacks.current.onChanged()
    }
    try {
      const next = await operatePluginAccount(
        { ...s.callbacks.current.input, operation },
        s.transport
      )
      if (!lease.alive || lease.generation !== generation) return false
      if (poll && Date.now() >= lease.deadline) {
        s.update({ pending: false, expired: true, status: null })
        return false
      }
      await acceptResult(s, { next, operation, poll })
      return true
    } catch (cause) {
      if (lease.alive && lease.generation === generation)
        s.update({ error: message(cause), pending: false })
      return false
    } finally {
      if (lease.generation === generation) {
        lease.running = false
        if (lease.alive) s.update({ busy: false })
      }
    }
  }, [])
}

async function acceptResult(
  state: Actions,
  result: { next: PluginAccountStatus; operation: Operation; poll: boolean }
) {
  const { next, operation, poll } = result,
    lease = state.lease.current
  state.update({
    status:
      poll && next.authenticated !== true
        ? { ...next, authorizationUrl: lease.url }
        : next,
    error: "",
  })
  if (operation === "login") {
    lease.url = next.authorizationUrl
    lease.deadline = Date.now() + LIFETIME_MS
    state.update({ pending: true })
    if (next.authorizationUrl)
      await openUrl(next.authorizationUrl).catch((cause) => {
        if (lease.alive) state.update({ error: message(cause) })
      })
  }
  if (next.authenticated === true || operation === "logout") {
    lease.url = null
    state.update({ pending: false })
    state.callbacks.current.onChanged()
  }
}

function useAccountPolling(
  pending: boolean,
  act: (operation: Operation, poll?: boolean) => Promise<boolean>,
  context: Pick<Actions, "lease" | "update">
) {
  const callbacks = useRef(context)
  useEffect(() => {
    callbacks.current = context
  })
  useEffect(() => {
    if (!pending) return
    let timer: ReturnType<typeof setTimeout>,
      active = true
    const expire = () => {
      const { lease, update } = callbacks.current
      lease.current.generation++
      lease.current.url = null
      lease.current.running = false
      update({ pending: false, expired: true, status: null, busy: false })
    }
    const deadlineTimer = setTimeout(
      expire,
      Math.max(0, callbacks.current.lease.current.deadline - Date.now())
    )
    const poll = async () => {
      const { lease } = callbacks.current
      if (Date.now() >= lease.current.deadline) {
        expire()
        return
      }
      await act("status", true)
      if (active) timer = setTimeout(poll, POLL_MS)
    }
    timer = setTimeout(poll, POLL_MS)
    return () => {
      active = false
      clearTimeout(timer)
      clearTimeout(deadlineTimer)
    }
  }, [pending, act])
}

function useAccountExpiry(state: State, update: Update) {
  useEffect(() => {
    if (state.pending || !state.status) return
    const timer = setTimeout(() => update({ status: null }), STATE_TTL_MS)
    return () => clearTimeout(timer)
  }, [state.pending, state.status, update])
}
