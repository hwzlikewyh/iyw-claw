"use client"

import { useSyncExternalStore } from "react"
import type { InputDelivery } from "@/lib/types"

export type MessageQueueBehavior = InputDelivery

const STORAGE_KEY = "iyw-claw.message-queue-behavior"
const DEFAULT_BEHAVIOR: InputDelivery = "smart"

export function isMessageQueueBehavior(
  value: string | null
): value is InputDelivery {
  return value === "smart" || value === "interrupt" || value === "queue"
}

function readBehavior(): InputDelivery {
  const value = window.localStorage.getItem(STORAGE_KEY)
  return isMessageQueueBehavior(value) ? value : DEFAULT_BEHAVIOR
}

function subscribe(notify: () => void) {
  const onStorage = (event: StorageEvent) => {
    if (event.key === STORAGE_KEY || event.key === null) notify()
  }
  window.addEventListener("storage", onStorage)
  window.addEventListener(STORAGE_KEY, notify)
  return () => {
    window.removeEventListener("storage", onStorage)
    window.removeEventListener(STORAGE_KEY, notify)
  }
}

function setBehavior(next: InputDelivery) {
  window.localStorage.setItem(STORAGE_KEY, next)
  window.dispatchEvent(new Event(STORAGE_KEY))
}

export function useMessageQueuePreference() {
  const behavior = useSyncExternalStore(
    subscribe,
    readBehavior,
    () => DEFAULT_BEHAVIOR
  )
  return { behavior, setBehavior }
}
