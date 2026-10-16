"use client"

import { useEffect, useMemo, useRef, useState } from "react"
import { Hand } from "lucide-react"
import { useTranslations } from "next-intl"
import { toast } from "sonner"
import { useBrowser } from "@/contexts/browser-context"
import { useBrowserHost } from "@/hooks/use-browser-host"
import { browserApi } from "@/lib/browser-api"
import {
  deliverBrowserSession,
  getBrowserSessionTarget,
} from "@/lib/browser-session-handoff"
import { buildElementReference } from "@/components/chat/composer/element-reference"
import type {
  BrowserElementSummary,
  BrowserTabSnapshot,
} from "@/lib/browser-types"
import { BrowserElementPicker } from "./browser-element-picker"
import { BrowserCanvas } from "./browser-canvas"
import { BrowserNativeSurface } from "./browser-native-surface"
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
  const t = useTranslations("Browser")
  const enabled = kind === "detached" || isOpen
  const {
    host,
    windowLabel,
    error: hostError,
    retry: retryHost,
  } = useBrowserHost(kind, enabled)
  const creatingRef = useRef(false)
  const hostedTabRef = useRef(false)
  const closingWindowRef = useRef(false)
  const [selecting, setSelecting] = useState(false)
  const [selectionTabId, setSelectionTabId] = useState<string | undefined>()
  const selectionTarget = useRef<string | null>(null)
  const selectionRevision = useRef(0)
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
  const selectionKey = `${activeTabId}:${activeTab?.documentEpoch}`
  if (selectionTabId !== selectionKey) {
    setSelectionTabId(selectionKey)
    setSelecting(false)
  }
  const closeShell =
    kind === "docked"
      ? () => void closeBrowser()
      : windowLabel
        ? () =>
            void browserApi
              .closeWindowPreservingTabs(windowLabel)
              .catch(() => {})
        : undefined

  useEffect(
    () => () => {
      selectionRevision.current += 1
    },
    [selectionKey, enabled]
  )

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
    return (
      <BrowserStatus
        hostError={hostError}
        onRetry={!host || hostError ? retryHost : undefined}
      />
    )
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
  const cancelSelection = () => {
    selectionRevision.current += 1
    setSelecting(false)
  }
  const selectElement = async (element: BrowserElementSummary) => {
    const target = selectionTarget.current
    const revision = selectionRevision.current
    if (!activeTab || !target) return
    const accepted = await deliverBrowserSession(target, {
      kind: "reference",
      reference: buildElementReference(element, activeTab.url),
    })
    if (!accepted) {
      throw new Error(t("conversationUnavailable"))
    }
    if (revision !== selectionRevision.current) return
    cancelSelection()
    toast.success(t("elementAdded"))
  }
  const toggleSelection = async () => {
    if (selecting) {
      cancelSelection()
      return
    }
    const revision = ++selectionRevision.current
    try {
      const target = await getBrowserSessionTarget()
      if (revision !== selectionRevision.current) return
      selectionTarget.current = target
      setSelecting(true)
    } catch (error) {
      console.warn("[Browser] selection target unavailable", { error })
      toast.error(t("conversationUnavailable"))
    }
  }

  return (
    <section className="flex h-full min-h-0 flex-col overflow-hidden bg-background">
      <BrowserTabStrip
        host={host}
        tabs={tabs}
        activeTabId={activeTabId}
        claim={claim}
      />
      <BrowserToolbar
        key={activeTab?.browserTabId ?? "empty"}
        host={host}
        tab={activeTab}
        selecting={selecting}
        onToggleSelecting={() => void toggleSelection()}
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
      <div className="relative min-h-0 flex-1">
        {state.capability.engine?.kind === "webview" && activeTab ? (
          <BrowserNativeSurface
            key={`${activeTab.browserTabId}:${activeTab.documentEpoch}`}
            tab={activeTab}
            selecting={selecting}
          />
        ) : (
          <BrowserCanvas tab={activeTab} claim={claim} selecting={selecting} />
        )}
        {selecting && activeTab ? (
          <BrowserElementPicker
            key={selectionKey}
            tabId={activeTab.browserTabId}
            onPick={selectElement}
            onCancel={cancelSelection}
          />
        ) : null}
      </div>
      <BrowserPrompts
        key={dialog?.dialogId ?? chooser?.chooserId ?? "none"}
        dialog={dialog}
        chooser={chooser}
      />
    </section>
  )
}
