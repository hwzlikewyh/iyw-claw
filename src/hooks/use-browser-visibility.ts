"use client"

import { useSyncExternalStore } from "react"
import { browserApi } from "@/lib/browser-api"
import { getShellTransport, isDesktop } from "@/lib/transport"

const STORAGE_KEY = "workspace:browser-visible"
const UPDATED_EVENT = "workspace:browser-visibility-updated"
let visibility: boolean | undefined
let synchronization: Promise<void> | null = null
let eventRevision = 0

export function readBrowserVisibility(): boolean {
  if (typeof window === "undefined") return false
  if (visibility !== undefined) return visibility
  try {
    return window.localStorage.getItem(STORAGE_KEY) === "true"
  } catch {
    return false
  }
}

function acceptVisibility(visible: boolean): void {
  visibility = visible
  try {
    window.localStorage.setItem(STORAGE_KEY, String(visible))
  } catch (error) {
    console.warn("[Browser] failed to cache visibility preference", error)
  }
  window.dispatchEvent(new Event(UPDATED_EVENT))
}

export async function writeBrowserVisibility(visible: boolean): Promise<void> {
  if (!isDesktop()) return acceptVisibility(visible)
  await synchronizeVisibility()
  const revision = eventRevision
  const saved = await browserApi.syncVisibility(visible)
  if (eventRevision === revision) acceptVisibility(saved)
}

function synchronizeVisibility(): Promise<void> {
  if (synchronization) return synchronization
  synchronization = startSynchronization().catch((error) => {
    synchronization = null
    throw error
  })
  return synchronization
}

async function startSynchronization(): Promise<void> {
  const unsubscribe = await getShellTransport().subscribe<boolean>(
    "browser://visibility-changed",
    (visible) => {
      eventRevision += 1
      acceptVisibility(visible)
    }
  )
  try {
    const revision = eventRevision
    const saved = await browserApi.syncVisibility(readBrowserVisibility(), true)
    if (eventRevision === revision) acceptVisibility(saved)
  } catch (error) {
    unsubscribe()
    throw error
  }
}

function subscribe(onChange: () => void) {
  const onStorage = (event: StorageEvent) => {
    if (event.key !== null && event.key !== STORAGE_KEY) return
    visibility = undefined
    onChange()
  }
  window.addEventListener("storage", onStorage)
  window.addEventListener(UPDATED_EVENT, onChange)
  if (isDesktop()) {
    void synchronizeVisibility().catch((error) => {
      console.error(
        "[Browser] failed to synchronize visibility preference",
        error
      )
    })
  }
  return () => {
    window.removeEventListener("storage", onStorage)
    window.removeEventListener(UPDATED_EVENT, onChange)
  }
}

export function useBrowserVisibility(): boolean {
  return useSyncExternalStore(subscribe, readBrowserVisibility, () => false)
}
