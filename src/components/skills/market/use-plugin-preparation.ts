"use client"

import { useCallback, useEffect, useRef, useState } from "react"
import { getTransport } from "@/lib/transport"
import { extractAppCommandError } from "@/lib/app-error"
import {
  readPluginPreparation,
  changePluginPreparation,
  checkPluginPreparation,
  type PluginPreparation,
  type PluginConnectionCheck,
} from "@/lib/plugin-preparation"

const REFRESH_MS = 10_000
const CHECK_TTL_MS = 60_000
const errorText = (cause: unknown) =>
  extractAppCommandError(cause)?.message || String(cause)
type Ref<T> = { current: T }
interface Lifecycle {
  id: string
  conversationId: number | null
  transport: ReturnType<typeof getTransport>
  alive: Ref<boolean>
  running: Ref<boolean>
  generation: Ref<number>
  onView: (value: PluginPreparation) => void
  onConnection: (value: PluginConnectionCheck | null) => void
  onError: (value: string) => void
}

export function usePluginPreparation(
  id: string,
  conversationId: number | null
) {
  const [transport] = useState(getTransport)
  const [view, setView] = useState<PluginPreparation | null>(null)
  const [connection, setConnection] = useState<PluginConnectionCheck | null>(
    null
  )
  const [error, setError] = useState("")
  const [revision, setRevision] = useState(0)
  const alive = useRef(true),
    running = useRef(false),
    generation = useRef(0)
  const token = useRef<string | null>(null)
  const refresh = useCallback(() => {
    setError("")
    setRevision((value) => value + 1)
  }, [])
  const applyView = useCallback((value: PluginPreparation) => {
    if (token.current !== value.contextToken) setConnection(null)
    token.current = value.contextToken
    setView(value)
  }, [])
  const lifecycle: Lifecycle = {
    id,
    conversationId,
    transport,
    alive,
    running,
    generation,
    onView: applyView,
    onConnection: setConnection,
    onError: setError,
  }
  usePreparationRead(lifecycle, revision)
  useConnectionExpiry(connection, setConnection)
  const actions = usePreparationActions(lifecycle)
  return {
    view,
    connection,
    error,
    refresh,
    applyView,
    clearConnection: () => setConnection(null),
    ...actions,
  }
}

function useConnectionExpiry(
  value: PluginConnectionCheck | null,
  clear: (value: null) => void
) {
  useEffect(() => {
    if (!value) return
    const timer = setTimeout(() => clear(null), CHECK_TTL_MS)
    return () => clearTimeout(timer)
  }, [value, clear])
}

function usePreparationActions(input: Lifecycle) {
  const [busy, setBusy] = useState(false)
  const mutate = async (review: PluginPreparation, enabled: boolean | null) => {
    if (
      input.running.current ||
      input.conversationId == null ||
      !review.contextToken
    )
      return false
    input.generation.current += 1
    input.running.current = true
    setBusy(true)
    input.onError("")
    input.onConnection(null)
    const request = {
      marketSkillId: input.id,
      conversationId: input.conversationId,
      contextToken: review.contextToken,
    }
    try {
      if (enabled == null) {
        const result = await checkPluginPreparation(request, input.transport)
        if (input.alive.current) input.onConnection(result)
      } else {
        const result = await changePluginPreparation(
          request,
          enabled,
          input.transport
        )
        if (input.alive.current) input.onView(result)
      }
      return true
    } catch (cause) {
      if (input.alive.current) input.onError(errorText(cause))
      return false
    } finally {
      input.running.current = false
      if (input.alive.current) setBusy(false)
    }
  }
  return { busy, mutate }
}

function usePreparationRead(input: Lifecycle, revision: number) {
  const callbacks = useRef(input)
  useEffect(() => {
    callbacks.current = input
  })
  useEffect(() => {
    let active = true
    const { alive, running } = callbacks.current
    alive.current = true
    let timer: ReturnType<typeof setTimeout>
    const read = async () => {
      try {
        if (!running.current) {
          const generation = callbacks.current.generation.current
          const value = await readPluginPreparation(
            { marketSkillId: input.id, conversationId: input.conversationId },
            input.transport
          )
          if (
            active &&
            !running.current &&
            callbacks.current.generation.current === generation
          )
            callbacks.current.onView(value)
        }
      } catch (cause) {
        if (active) {
          callbacks.current.onConnection(null)
          callbacks.current.onError(errorText(cause))
        }
      }
      if (active) timer = setTimeout(read, REFRESH_MS)
    }
    void read()
    return () => {
      active = false
      alive.current = false
      clearTimeout(timer)
    }
  }, [input.id, input.conversationId, input.transport, revision])
}

export type PluginPreparationController = ReturnType<
  typeof usePluginPreparation
>
