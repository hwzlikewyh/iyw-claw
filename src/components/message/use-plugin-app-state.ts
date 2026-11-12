"use client"

import { useCallback, useEffect, useRef, useState } from "react"
import {
  pluginAppOpen,
  type PluginAppLaunch,
  type PluginAppResourceMeta,
} from "@/lib/api"
import { getTransport, onTransportChange } from "@/lib/transport"

type AppState =
  | { status: "loading" }
  | { status: "error"; message: string }
  | {
      status: "ready"
      html: string
      launch: PluginAppLaunch
      hostVersion: string
      resourceMeta?: PluginAppResourceMeta
    }

export function usePluginAppState(
  instanceId: string,
  conversationId: number,
  unavailable: string
) {
  const [state, setState] = useState<AppState>({ status: "loading" })
  const [fullscreen, setFullscreen] = useState(false)
  const generation = useRef({ value: 0 })
  const alive = useRef(true)
  const retry = useCallback(async () => {
    const current = ++generation.current.value
    setState({ status: "loading" })
    try {
      const response = await pluginAppOpen({ instanceId, conversationId })
      if (!alive.current || generation.current.value !== current) return
      setFullscreen(response.launch.displayMode === "fullscreen")
      setState({
        status: "ready",
        html: response.html,
        launch: response.launch,
        hostVersion: response.hostVersion,
        resourceMeta: response.resourceMeta,
      })
    } catch (cause) {
      if (alive.current && generation.current.value === current)
        setState({
          status: "error",
          message: cause instanceof Error ? cause.message : unavailable,
        })
    }
  }, [instanceId, conversationId, unavailable])
  useInitialApp(retry, alive, generation)
  const revoke = useCallback(() => {
    generation.current.value++
    setFullscreen(false)
    setState({ status: "error", message: unavailable })
  }, [unavailable])
  useAppRevocation(instanceId, revoke)
  return { state, fullscreen, setFullscreen, retry }
}

function useInitialApp(
  retry: () => Promise<void>,
  aliveRef: { current: boolean },
  generationRef: { current: { value: number } }
) {
  useEffect(() => {
    let active = true
    const counter = generationRef.current
    aliveRef.current = true
    void Promise.resolve().then(() => {
      if (active) void retry()
    })
    return () => {
      active = false
      aliveRef.current = false
      counter.value++
    }
  }, [retry, aliveRef, generationRef])
}

function useAppRevocation(instanceId: string, revoke: () => void) {
  useEffect(() => {
    let active = true
    let unlisten: (() => void) | undefined
    void getTransport()
      .subscribe<{ instanceIds: string[] }>(
        "plugin-app://revoked",
        (payload) => {
          if (active && payload.instanceIds.includes(instanceId)) revoke()
        }
      )
      .then((dispose) => {
        if (active) unlisten = dispose
        else dispose()
      })
      .catch(() => {
        if (active) revoke()
      })
    const untransport = onTransportChange(revoke)
    const unreconnect = getTransport().onReconnect?.(revoke)
    return () => {
      active = false
      unlisten?.()
      untransport()
      unreconnect?.()
    }
  }, [instanceId, revoke])
}
