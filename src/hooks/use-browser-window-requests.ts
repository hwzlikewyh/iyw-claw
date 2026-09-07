"use client"

import { useEffect, useRef } from "react"
import { browserApi } from "@/lib/browser-api"
import type { BrowserStateSnapshot } from "@/lib/browser-types"
import { isDesktop } from "@/lib/platform"
import { readBrowserVisibility } from "./use-browser-visibility"

interface WindowRequest {
  requestId: string
  browserTabId: string
}

interface WindowRequestsOptions {
  enabled: boolean
  state: BrowserStateSnapshot | null
  acceptState: (state: BrowserStateSnapshot) => void
  detachTab: (tabId: string, sourceHostId?: string) => Promise<void>
}

export function useBrowserWindowRequests({
  enabled,
  state,
  acceptState,
  detachTab,
}: WindowRequestsOptions) {
  useWindowRequest({
    enabled,
    requests: state?.userActionRequests,
    operation: async (request) => {
      await openTabWindow(request.browserTabId, detachTab)
    },
  })
  useWindowRequest({
    enabled,
    requests: state?.windowOpenRequests,
    operation: async (request) => {
      if (await openTabWindow(request.browserTabId, detachTab)) {
        acceptState(await browserApi.completeWindowOpen(request.requestId))
      }
    },
  })
  useWindowRequest({
    enabled,
    requests: state?.windowCloseRequests,
    operation: async (request) => {
      const latest = await browserApi.state()
      const tab = latest.tabs.find(
        (item) => item.browserTabId === request.browserTabId
      )
      const host = latest.hosts.find(
        (item) => item.hostId === tab?.hostId && item.kind === "detached"
      )
      if (host) await browserApi.closeWindowPreservingTabs(host.windowLabel)
      acceptState(await browserApi.completeWindowClose(request.requestId))
    },
  })
}

async function openTabWindow(
  tabId: string,
  detachTab: WindowRequestsOptions["detachTab"]
): Promise<boolean> {
  const latest = await browserApi.state()
  // 排队期间也可能从设置窗口关闭开关，执行前重新读取持久化值。
  if (!readBrowserVisibility()) return false
  const tab = latest.tabs.find((item) => item.browserTabId === tabId)
  if (!tab) return false
  const host = latest.hosts.find(
    (item) => item.hostId === tab.hostId && item.kind === "detached"
  )
  if (host) await browserApi.focusWindow(host.windowLabel)
  else await detachTab(tabId, tab.hostId)
  return readBrowserVisibility()
}

function useWindowRequest({
  enabled,
  requests,
  operation,
}: {
  enabled: boolean
  requests: WindowRequest[] | undefined
  operation: (request: WindowRequest) => Promise<void>
}) {
  const handledRef = useRef(new Set<string>())
  const loggedFailuresRef = useRef(new Set<string>())
  useEffect(() => {
    if (!enabled || !isDesktop() || !requests) return
    const handled = handledRef.current
    const activeIds = new Set(requests.map((request) => request.requestId))
    for (const requestId of handled) {
      if (!activeIds.has(requestId)) handled.delete(requestId)
    }
    for (const requestId of loggedFailuresRef.current) {
      if (!activeIds.has(requestId)) loggedFailuresRef.current.delete(requestId)
    }
    const request = requests.find((item) => !handled.has(item.requestId))
    if (!request) return
    handled.add(request.requestId)
    void queueWindowRequest(async () => {
      if (!readBrowserVisibility()) {
        handled.delete(request.requestId)
        return
      }
      await operation(request)
      if (!readBrowserVisibility()) handled.delete(request.requestId)
    }).catch((error) => {
      handled.delete(request.requestId)
      if (
        readBrowserVisibility() &&
        !loggedFailuresRef.current.has(request.requestId)
      ) {
        loggedFailuresRef.current.add(request.requestId)
        console.error("[Browser] window request failed", error)
      }
    })
  }, [enabled, operation, requests])
}

let windowRequestTransition = Promise.resolve()

function queueWindowRequest(operation: () => Promise<void>) {
  windowRequestTransition = windowRequestTransition.then(operation, operation)
  return windowRequestTransition
}
