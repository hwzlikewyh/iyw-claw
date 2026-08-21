"use client"

import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react"
import { browserApi } from "@/lib/browser-api"
import type {
  BrowserErrorEnvelope,
  BrowserStateSnapshot,
} from "@/lib/browser-types"
import { isDesktop } from "@/lib/platform"

const POLL_INTERVAL_MS = 800
const DETACHED_HOST_TIMEOUT_MS = 10_000

interface BrowserContextValue {
  isOpen: boolean
  state: BrowserStateSnapshot | null
  error: BrowserErrorEnvelope | null
  busy: boolean
  openBrowser: () => Promise<void>
  closeBrowser: () => Promise<void>
  toggleBrowser: () => Promise<void>
  refresh: () => Promise<BrowserStateSnapshot | null>
  acceptState: (state: BrowserStateSnapshot) => void
  run: (operation: () => Promise<BrowserStateSnapshot>) => Promise<void>
  detachTab: (tabId: string, sourceHostId?: string) => Promise<void>
}

const BrowserContext = createContext<BrowserContextValue | null>(null)

export function BrowserProvider({
  children,
  defaultOpen = false,
}: {
  children: React.ReactNode
  defaultOpen?: boolean
}) {
  const [isOpen, setOpen] = useState(defaultOpen)
  const [state, setState] = useState<BrowserStateSnapshot | null>(null)
  const [error, setError] = useState<BrowserErrorEnvelope | null>(null)
  const [busy, setBusy] = useState(false)
  const mountedRef = useRef(true)
  const acceptedRevisionRef = useRef(0)
  const refreshPromiseRef = useRef<Promise<BrowserStateSnapshot | null> | null>(
    null
  )

  const acceptState = useCallback((next: BrowserStateSnapshot) => {
    if (!mountedRef.current || next.stateRevision < acceptedRevisionRef.current)
      return
    acceptedRevisionRef.current = next.stateRevision
    setState(next)
    setError(null)
  }, [])

  const refresh = useCallback(async () => {
    if (!isDesktop()) return null
    if (refreshPromiseRef.current) return refreshPromiseRef.current
    const request = browserApi
      .state()
      .then((next) => {
        acceptState(next)
        return next
      })
      .catch((cause) => {
        if (mountedRef.current) setError(normalizeError(cause))
        return null
      })
      .finally(() => {
        if (refreshPromiseRef.current === request) {
          refreshPromiseRef.current = null
        }
      })
    refreshPromiseRef.current = request
    return request
  }, [acceptState])

  const run = useCallback(
    async (operation: () => Promise<BrowserStateSnapshot>) => {
      setBusy(true)
      try {
        acceptState(await operation())
      } catch (cause) {
        setError(normalizeError(cause))
      } finally {
        if (mountedRef.current) setBusy(false)
      }
    },
    [acceptState]
  )

  const openBrowser = useCallback(async () => {
    setOpen(true)
    setError(null)
    if (!isDesktop()) return
    setBusy(true)
    try {
      acceptState(await browserApi.start())
    } catch (cause) {
      setError(normalizeError(cause))
    } finally {
      if (mountedRef.current) setBusy(false)
    }
  }, [acceptState])

  const closeBrowser = useCallback(async () => {
    setOpen(false)
    setError(null)
    setBusy(false)
  }, [])
  const toggleBrowser = useCallback(async () => {
    if (isOpen) await closeBrowser()
    else await openBrowser()
  }, [closeBrowser, isOpen, openBrowser])

  const detachTab = useCallback(
    async (tabId: string, sourceHostId?: string) => {
      setBusy(true)
      let label: string | null = null
      const sourceIsDocked = state?.hosts.some(
        (host) => host.hostId === sourceHostId && host.kind === "docked"
      )
      try {
        label = await browserApi.createWindow()
        const host = await waitForHost(label, refresh)
        await browserApi.beginClaim(tabId, sourceHostId, host.hostId, 0)
        await waitForTabHost(tabId, host.hostId, refresh)
        if (sourceIsDocked) setOpen(false)
      } catch (cause) {
        if (label) await browserApi.closeWindow(label).catch(() => {})
        setError(normalizeError(cause))
        throw cause
      } finally {
        if (mountedRef.current) setBusy(false)
      }
    },
    [refresh, state?.hosts]
  )

  useEffect(() => {
    mountedRef.current = true
    if (isDesktop())
      void browserApi
        .refreshCapability()
        .then(acceptState)
        .catch(() => {})
    return () => {
      mountedRef.current = false
    }
  }, [acceptState])

  useEffect(() => {
    if (!isOpen || !isDesktop()) return
    let cancelled = false
    let polling = false
    let timer: number | null = null
    const schedule = (delay: number) => {
      if (cancelled || document.visibilityState !== "visible") return
      timer = window.setTimeout(poll, delay)
    }
    const poll = async () => {
      timer = null
      if (cancelled || polling || document.visibilityState !== "visible") return
      polling = true
      await refresh()
      polling = false
      schedule(POLL_INTERVAL_MS)
    }
    const handleVisibilityChange = () => {
      if (timer !== null) window.clearTimeout(timer)
      timer = null
      if (document.visibilityState === "visible") void poll()
    }
    document.addEventListener("visibilitychange", handleVisibilityChange)
    schedule(POLL_INTERVAL_MS)
    return () => {
      cancelled = true
      if (timer !== null) window.clearTimeout(timer)
      document.removeEventListener("visibilitychange", handleVisibilityChange)
    }
  }, [isOpen, refresh])

  const value = useMemo<BrowserContextValue>(
    () => ({
      isOpen,
      state,
      error,
      busy,
      openBrowser,
      closeBrowser,
      toggleBrowser,
      refresh,
      acceptState,
      run,
      detachTab,
    }),
    [
      isOpen,
      state,
      error,
      busy,
      openBrowser,
      closeBrowser,
      toggleBrowser,
      refresh,
      acceptState,
      run,
      detachTab,
    ]
  )

  return (
    <BrowserContext.Provider value={value}>{children}</BrowserContext.Provider>
  )
}

export function useBrowser() {
  const value = useContext(BrowserContext)
  if (!value) throw new Error("useBrowser must be used within BrowserProvider")
  return value
}

async function waitForHost(
  windowLabel: string,
  refresh: () => Promise<BrowserStateSnapshot | null>
) {
  const deadline = Date.now() + DETACHED_HOST_TIMEOUT_MS
  while (Date.now() < deadline) {
    const state = await refresh()
    const host = state?.hosts.find((item) => item.windowLabel === windowLabel)
    if (host) return host
    await new Promise((resolve) => window.setTimeout(resolve, 100))
  }
  throw new Error("Detached browser window did not register")
}

async function waitForTabHost(
  tabId: string,
  hostId: string,
  refresh: () => Promise<BrowserStateSnapshot | null>
) {
  const deadline = Date.now() + DETACHED_HOST_TIMEOUT_MS
  while (Date.now() < deadline) {
    const state = await refresh()
    const tab = state?.tabs.find((item) => item.browserTabId === tabId)
    if (tab?.hostId === hostId) return
    await new Promise((resolve) => window.setTimeout(resolve, 100))
  }
  throw new Error("Detached browser tab did not migrate")
}

function normalizeError(cause: unknown): BrowserErrorEnvelope {
  const value = cause as Partial<BrowserErrorEnvelope> | null
  return {
    code: value?.code ?? "BROWSER_INTERNAL",
    message: value?.message ?? String(cause),
    retryable: value?.retryable ?? false,
    effectMayHaveOccurred: value?.effectMayHaveOccurred ?? false,
  }
}
