"use client"

import { useSyncExternalStore } from "react"
import { getTransport, onTransportChange } from "@/lib/transport"
import {
  getUiSettings,
  setUiSettings,
  UI_SETTINGS_EVENT,
  type UiSettings,
} from "./api"

type Snapshot = UiSettings & { loaded: boolean; error: string | null }
const fallback: Snapshot = { enabled: false, loaded: false, error: null }
let snapshot = fallback
let loading: Promise<void> | null = null
let dispose: (() => void) | null = null
let generation = 0,
  version = 0
const listeners = new Set<() => void>()
export const uiEnabled = () => snapshot.enabled && snapshot.loaded
const notify = () => listeners.forEach((listener) => listener())
function accept(settings: UiSettings) {
  version++
  snapshot = { ...settings, loaded: true, error: null }
  notify()
}

export function reloadUiSettings() {
  if (loading) return loading
  const transport = getTransport(),
    captured = version
  const request = getUiSettings()
    .then((value) => {
      if (transport === getTransport() && captured === version) accept(value)
    })
    .catch((error) => {
      if (transport !== getTransport() || captured !== version) return
      snapshot = { ...fallback, loaded: true, error: String(error) }
      notify()
    })
    .finally(() => {
      if (loading === request) loading = null
    })
  loading = request
  return request
}
function connect() {
  dispose?.()
  const current = ++generation,
    transport = getTransport()
  const reconnect = transport.onReconnect?.(() => void reloadUiSettings())
  dispose = () => {
    generation++
    reconnect?.()
  }
  void transport
    .subscribe<UiSettings>(UI_SETTINGS_EVENT, (value) => {
      if (current === generation) accept(value)
    })
    .then((unsub) => {
      if (current !== generation) {
        unsub()
        return
      }
      dispose = () => {
        generation++
        unsub()
        reconnect?.()
      }
    })
    .catch(() => {})
  void reloadUiSettings()
}
function subscribe(listener: () => void) {
  listeners.add(listener)
  if (listeners.size === 1) connect()
  return () => {
    listeners.delete(listener)
    if (!listeners.size) {
      dispose?.()
      dispose = null
    }
  }
}
export function useUiSettings() {
  return useSyncExternalStore(
    subscribe,
    () => snapshot,
    () => fallback
  )
}
export async function updateUiSettings(enabled: boolean) {
  const transport = getTransport()
  if (!enabled && uiEnabled()) {
    const { flushAllUiStates } = await import("./state-store")
    await flushAllUiStates().catch(() => {})
  }
  if (transport !== getTransport()) return
  const captured = version
  const value = await setUiSettings({ enabled })
  if (
    transport === getTransport() &&
    (captured === version || snapshot.enabled === value.enabled)
  )
    accept(value)
}
onTransportChange(() => {
  dispose?.()
  dispose = null
  loading = null
  version++
  snapshot = fallback
  notify()
  if (listeners.size) connect()
})
