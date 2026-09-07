"use client"

import { useSyncExternalStore } from "react"

const STORAGE_KEY = "workspace:browser-visible"
const UPDATED_EVENT = "workspace:browser-visibility-updated"

export function readBrowserVisibility(): boolean {
  if (typeof window === "undefined") return false
  try {
    return window.localStorage.getItem(STORAGE_KEY) === "true"
  } catch {
    return false
  }
}

export function writeBrowserVisibility(visible: boolean): void {
  window.localStorage.setItem(STORAGE_KEY, String(visible))
  window.dispatchEvent(new Event(UPDATED_EVENT))
  console.info("[Browser] visibility preference saved", { visible })
}

function subscribe(onChange: () => void) {
  const onStorage = (event: StorageEvent) => {
    if (event.key === null || event.key === STORAGE_KEY) onChange()
  }
  window.addEventListener("storage", onStorage)
  window.addEventListener(UPDATED_EVENT, onChange)
  return () => {
    window.removeEventListener("storage", onStorage)
    window.removeEventListener(UPDATED_EVENT, onChange)
  }
}

export function useBrowserVisibility(): boolean {
  return useSyncExternalStore(subscribe, readBrowserVisibility, () => false)
}
