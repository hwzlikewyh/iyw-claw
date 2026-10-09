import { useSyncExternalStore } from "react"
import { subscribe } from "@/lib/platform"
import { onTransportChange } from "@/lib/transport"
import {
  askComputerServed,
  computerAvailable,
  subscribeComputerServed,
} from "./computer-api"
import {
  COMPUTER_ACTIVITY_EVENT,
  COMPUTER_BACKEND_STATUS_EVENT,
  COMPUTER_STATE_EVENT,
  type ActivityOutcome,
  type BackendStatus,
  type ComputerAction,
  type ComputerActivityPayload,
  type ComputerStatePayload,
  type SharedApp,
  type SharedScreen,
  type SharedWindow,
} from "./types"
export interface ComputerActivityLine {
  actor?: ComputerActivityPayload["actor"]
  targetId: string
  app?: string
  action: ComputerAction
  outcome: ActivityOutcome
  at: number
  count: number
}
export interface ComputerStoreState {
  shared: readonly SharedWindow[]
  sharedApps: readonly SharedApp[]
  sharedScreen: SharedScreen | null
  sharedKnown: boolean
  backend: BackendStatus | null
  activity: readonly ComputerActivityLine[]
}
const ACTIVITY_LIMIT = 50
let state: ComputerStoreState = {
  shared: [],
  sharedApps: [],
  sharedScreen: null,
  sharedKnown: false,
  backend: null,
  activity: [],
}
const listeners = new Set<() => void>()
let started = false
let awaitingServed = false
let sharedVersion = 0
let backendVersion = 0
let subscriptionGeneration = 0
let unsubscribers: (() => void)[] = []
function emit(next: ComputerStoreState) {
  state = next
  for (const listener of listeners) listener()
}
export function setComputerShared(
  shared: readonly SharedWindow[],
  sharedApps?: readonly SharedApp[],
  sharedScreen?: SharedScreen | null
): void {
  sharedVersion += 1
  emit({
    ...state,
    shared,
    sharedApps: sharedApps ?? state.sharedApps,
    sharedScreen:
      sharedScreen === undefined ? state.sharedScreen : sharedScreen,
    sharedKnown: true,
  })
}
export function setComputerBackend(backend: BackendStatus): void {
  backendVersion += 1
  emit({ ...state, backend })
}
export interface ComputerStoreMark {
  shared: number
  backend: number
}
export function computerStoreMark(): ComputerStoreMark {
  return { shared: sharedVersion, backend: backendVersion }
}
export function setComputerSharedSince(
  shared: readonly SharedWindow[],
  mark: ComputerStoreMark
): void {
  if (sharedVersion === mark.shared) setComputerShared(shared)
}
export function setComputerStateSince(
  next: ComputerStatePayload,
  mark: ComputerStoreMark
): void {
  if (sharedVersion === mark.shared)
    setComputerShared(next.shared, next.apps ?? [], next.screen ?? null)
}
export function setComputerBackendSince(
  backend: BackendStatus,
  mark: ComputerStoreMark
): void {
  if (backendVersion === mark.backend) setComputerBackend(backend)
}
export function recordComputerActivity(payload: ComputerActivityPayload): void {
  const head = state.activity[0]
  const activity =
    head &&
    head.targetId === payload.targetId &&
    head.app === payload.app &&
    head.actor?.connectionId === payload.actor?.connectionId &&
    head.action === payload.action &&
    head.outcome === payload.outcome
      ? [
          { ...head, at: payload.at, count: head.count + 1 },
          ...state.activity.slice(1),
        ]
      : [
          { ...payload, count: 1 },
          ...state.activity.slice(0, ACTIVITY_LIMIT - 1),
        ]
  emit({ ...state, activity })
}
export function clearComputerActivity(): void {
  if (state.activity.length === 0) return
  emit({ ...state, activity: [] })
}
function ensureStarted() {
  if (started) return
  if (!computerAvailable()) {
    if (!awaitingServed) {
      awaitingServed = true
      subscribeComputerServed(() => {
        if (computerAvailable()) ensureStarted()
      })
    }
    void askComputerServed()
    return
  }
  started = true
  attach<ComputerStatePayload>(COMPUTER_STATE_EVENT, (p) =>
    setComputerShared(p.shared, p.apps ?? [], p.screen ?? null)
  )
  attach<ComputerActivityPayload>(
    COMPUTER_ACTIVITY_EVENT,
    recordComputerActivity
  )
  attach<BackendStatus>(COMPUTER_BACKEND_STATUS_EVENT, setComputerBackend)
}

function attach<T>(event: string, listener: (payload: T) => void) {
  const generation = subscriptionGeneration
  void subscribe<T>(event, (payload) => {
    if (generation === subscriptionGeneration) listener(payload)
  })
    .then((unsubscribe) => {
      if (generation === subscriptionGeneration) unsubscribers.push(unsubscribe)
      else unsubscribe()
    })
    .catch(() => {})
}

onTransportChange(() => {
  subscriptionGeneration += 1
  for (const unsubscribe of unsubscribers) unsubscribe()
  unsubscribers = []
  started = false
  sharedVersion += 1
  backendVersion += 1
  emit({
    shared: [],
    sharedApps: [],
    sharedScreen: null,
    sharedKnown: false,
    backend: null,
    activity: [],
  })
  ensureStarted()
})
function subscribeStore(listener: () => void): () => void {
  ensureStarted()
  listeners.add(listener)
  return () => listeners.delete(listener)
}
export function useComputerStore(): ComputerStoreState {
  return useSyncExternalStore(
    subscribeStore,
    () => state,
    () => state
  )
}
export function resetComputerStoreForTest(): void {
  state = {
    shared: [],
    sharedApps: [],
    sharedScreen: null,
    sharedKnown: false,
    backend: null,
    activity: [],
  }
}
