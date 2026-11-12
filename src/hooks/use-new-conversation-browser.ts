"use client"

import { useEffect, useRef, type RefObject } from "react"
import { useAuxPanelContext } from "@/contexts/aux-panel-context"
import { useBrowser } from "@/contexts/browser-context"
import { useTabStore } from "@/contexts/tab-context"
import { useWorkbenchRoute } from "@/contexts/workbench-route-context"
import { browserApi } from "@/lib/browser-api"
import { DEFAULT_BROWSER_HOME_URL } from "@/lib/browser-defaults"
import { normalizeBrowserError } from "@/lib/browser-startup"
import { isLocalDesktop } from "@/lib/platform"
import { useBrowserVisibility } from "./use-browser-visibility"
import { useIsMobile } from "./use-mobile"

interface DraftBrowserRequest {
  draftId: string
  hadTabs: boolean
}

export function useNewConversationBrowser() {
  const draftId = useTabStore((store) => {
    const tab = store.tabs.find((item) => item.id === store.activeTabId)
    return tab?.conversationId === null ? tab.id : null
  })
  const { state, openBrowser } = useBrowser()
  const { setOpen } = useAuxPanelContext()
  const { isConversations } = useWorkbenchRoute()
  const isMobile = useIsMobile()
  const visible = useBrowserVisibility()
  const initialized = useRef(new Set<string>())
  const pendingRef = useRef<DraftBrowserRequest | null>(null)
  useEffect(() => {
    if (pendingRef.current?.draftId !== draftId) pendingRef.current = null
    if (
      !draftId ||
      !isConversations ||
      isMobile ||
      !isLocalDesktop() ||
      !visible ||
      !state?.capability.supported ||
      initialized.current.has(draftId)
    )
      return
    // 每个草稿只初始化一次，关闭或切回同一草稿不会强制重新展开。
    initialized.current.add(draftId)
    pendingRef.current = { draftId, hadTabs: state.tabs.length > 0 }
    setOpen(false)
    console.info("[Browser] opening new conversation tab", { draftId })
    void openBrowser()
  }, [draftId, isConversations, isMobile, openBrowser, setOpen, state, visible])
  useDraftBrowserTab(pendingRef, draftId)
}

function useDraftBrowserTab(
  pendingRef: RefObject<DraftBrowserRequest | null>,
  draftId: string | null
) {
  const { state, isOpen, run } = useBrowser()
  useEffect(() => {
    const request = pendingRef.current
    if (
      !isOpen ||
      !request ||
      request.draftId !== draftId ||
      state?.runtime.status !== "running"
    )
      return
    const host = state.hosts.find(
      (item) => item.kind === "docked" && item.windowLabel === "main"
    )
    if (!host) return
    pendingRef.current = null
    void run(() => initializeDraftBrowserTab(request, host.hostId))
  }, [draftId, isOpen, pendingRef, run, state])
}

async function initializeDraftBrowserTab(
  request: DraftBrowserRequest,
  hostId: string
) {
  const context = { draftId: request.draftId, hostId, hadTabs: request.hadTabs }
  try {
    const next = await (request.hadTabs
      ? browserApi.createTab(DEFAULT_BROWSER_HOME_URL, hostId)
      : browserApi.ensureInitialTab(DEFAULT_BROWSER_HOME_URL, hostId))
    console.info("[Browser] new conversation tab ready", {
      ...context,
      tabCount: next.tabs.length,
    })
    return next
  } catch (cause) {
    console.error("[Browser] new conversation tab failed", {
      ...context,
      ...normalizeBrowserError(cause),
    })
    throw cause
  }
}
