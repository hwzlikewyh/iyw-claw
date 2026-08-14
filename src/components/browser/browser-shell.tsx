"use client"

import { useEffect, useMemo, useRef } from "react"
import { useBrowser } from "@/contexts/browser-context"
import { useBrowserHost } from "@/hooks/use-browser-host"
import { browserApi } from "@/lib/browser-api"
import type { AgentAccess, BrowserTabSnapshot } from "@/lib/browser-types"
import { useTabStore } from "@/stores/tab-store"
import { BrowserCanvas } from "./browser-canvas"
import { BrowserPrompts } from "./browser-prompts"
import { BrowserStatus } from "./browser-status"
import { BrowserTabStrip } from "./browser-tab-strip"
import { BrowserToolbar } from "./browser-toolbar"

export function BrowserShell({
  kind = "docked",
}: {
  kind?: "docked" | "detached"
}) {
  const { state, isOpen, closeBrowser, run } = useBrowser()
  const activeConversationId = useTabStore((store) => {
    const active = store.rawTabs.find((tab) => tab.id === store.activeTabId)
    return active?.conversationId ?? null
  })
  const defaultAccess = useMemo<AgentAccess>(
    () =>
      activeConversationId
        ? {
            kind: "shared_conversation",
            conversationId: activeConversationId,
          }
        : { kind: "user_only" },
    [activeConversationId]
  )
  const enabled = kind === "detached" || isOpen
  const { host } = useBrowserHost(kind, enabled)
  const creatingRef = useRef(false)
  const claim = state?.viewClaims.find(
    (item) => item.targetHostId === host?.hostId
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
      browserApi.createTab("about:blank", defaultAccess, host.hostId)
    ).finally(() => {
      creatingRef.current = false
    })
  }, [defaultAccess, host, kind, run, state])

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

  return (
    <section className="flex h-full min-h-0 flex-col overflow-hidden bg-background">
      <BrowserTabStrip
        host={host}
        tabs={tabs}
        activeTabId={activeTabId}
        claim={claim}
        newTabAccess={defaultAccess}
      />
      <BrowserToolbar
        key={`${activeTab?.browserTabId ?? "empty"}:${activeTab?.url ?? ""}`}
        host={host}
        tab={activeTab}
        sharedAccess={defaultAccess}
        onClose={kind === "docked" ? closeBrowser : undefined}
      />
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
