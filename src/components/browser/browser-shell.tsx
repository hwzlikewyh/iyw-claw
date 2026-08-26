"use client"

import { useEffect, useMemo, useRef } from "react"
import { Hand } from "lucide-react"
import { useBrowser } from "@/contexts/browser-context"
import { useBrowserHost } from "@/hooks/use-browser-host"
import { browserApi } from "@/lib/browser-api"
import type { BrowserTabSnapshot } from "@/lib/browser-types"
import { BrowserCanvas } from "./browser-canvas"
import { BrowserPrompts } from "./browser-prompts"
import { BrowserStatus } from "./browser-status"
import { BrowserTabStrip } from "./browser-tab-strip"
import { BrowserToolbar } from "./browser-toolbar"
import { DEFAULT_BROWSER_HOME_URL } from "@/lib/browser-defaults"

export function BrowserShell({
  kind = "docked",
}: {
  kind?: "docked" | "detached"
}) {
  const { state, isOpen, closeBrowser, run } = useBrowser()
  const enabled = kind === "detached" || isOpen
  const { host, windowLabel } = useBrowserHost(kind, enabled)
  const creatingRef = useRef(false)
  const hostedTabRef = useRef(false)
  const closingWindowRef = useRef(false)
  const claim = state?.viewClaims.find(
    (item) => item.targetHostId === host?.hostId
  )
  const hostHasClaim = state?.viewClaims.some(
    (item) =>
      item.sourceHostId === host?.hostId || item.targetHostId === host?.hostId
  )

  const tabs = useMemo(() => {
    if (!state || !host) return []
    const byId = new Map(state.tabs.map((tab) => [tab.browserTabId, tab]))
    const ordered = host.tabOrder
      .map((tabId) => byId.get(tabId))
      .filter((tab): tab is BrowserTabSnapshot => Boolean(tab))
    if (
      claim &&
      !ordered.some((tab) => tab.browserTabId === claim.browserTabId)
    ) {
      const claimed = byId.get(claim.browserTabId)
      if (claimed)
        ordered.splice(Math.min(claim.targetIndex, ordered.length), 0, claimed)
    }
    return ordered
  }, [claim, host, state])

  const activeTabId =
    claim?.browserTabId ?? host?.activeTabId ?? tabs[0]?.browserTabId
  const activeTab = tabs.find((tab) => tab.browserTabId === activeTabId) ?? null
  const closeShell =
    kind === "docked"
      ? () => void closeBrowser()
      : windowLabel
        ? () => void browserApi.closeWindow(windowLabel).catch(() => {})
        : undefined

  useEffect(() => {
    if (kind !== "detached" || !host || !windowLabel) return
    if (tabs.length > 0 || hostHasClaim) {
      hostedTabRef.current = true
      return
    }
    if (!hostedTabRef.current || closingWindowRef.current) return
    closingWindowRef.current = true
    void browserApi.closeWindow(windowLabel).catch(() => {
      closingWindowRef.current = false
    })
  }, [host, hostHasClaim, kind, tabs.length, windowLabel])

  useEffect(() => {
    if (
      kind !== "docked" ||
      !host ||
      state?.runtime.status !== "running" ||
      state.tabs.length > 0 ||
      creatingRef.current
    ) {
      return
    }
    creatingRef.current = true
    void run(() =>
      browserApi.ensureInitialTab(DEFAULT_BROWSER_HOME_URL, host.hostId)
    ).finally(() => {
      creatingRef.current = false
    })
  }, [host, kind, run, state])

  if (!enabled) return null
  if (!state || !host || state.runtime.status !== "running") {
    return <BrowserStatus />
  }

  const dialog = state.dialogs.find(
    (item) => item.browserTabId === activeTab?.browserTabId
  )
  const chooser = state.fileChoosers.find(
    (item) => item.browserTabId === activeTab?.browserTabId
  )
  const userActionRequest = state.userActionRequests.find(
    (item) => item.browserTabId === activeTab?.browserTabId
  )

  return (
    <section className="flex h-full min-h-0 flex-col overflow-hidden bg-background">
      <BrowserTabStrip
        host={host}
        tabs={tabs}
        activeTabId={activeTabId}
        claim={claim}
      />
      <BrowserToolbar
        key={`${activeTab?.browserTabId ?? "empty"}:${activeTab?.url ?? ""}`}
        host={host}
        tab={activeTab}
        onClose={closeShell}
      />
      {userActionRequest ? (
        <div
          className="flex shrink-0 items-start gap-2 border-b bg-muted/50 px-3 py-2 text-xs text-foreground"
          aria-live="polite"
        >
          <Hand className="mt-0.5 size-3.5 shrink-0 text-muted-foreground" />
          <span className="min-w-0 break-words">
            {userActionRequest.reason}
          </span>
        </div>
      ) : null}
      <div className="min-h-0 flex-1">
        <BrowserCanvas tab={activeTab} claim={claim} />
      </div>
      <BrowserPrompts
        key={dialog?.dialogId ?? chooser?.chooserId ?? "none"}
        dialog={dialog}
        chooser={chooser}
      />
    </section>
  )
}
