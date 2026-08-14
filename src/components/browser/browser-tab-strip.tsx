"use client"

import { useRef, useState } from "react"
import { Globe2, Plus, X } from "lucide-react"
import { useTranslations } from "next-intl"
import { browserApi } from "@/lib/browser-api"
import type {
  AgentAccess,
  BrowserHostSnapshot,
  BrowserTabSnapshot,
  BrowserViewClaimSnapshot,
} from "@/lib/browser-types"
import { useBrowser } from "@/contexts/browser-context"
import { Button } from "@/components/ui/button"
import { cn } from "@/lib/utils"

const TAB_DRAG_MIME = "application/x-iyw-claw-browser-tab"

export function BrowserTabStrip({
  host,
  tabs,
  activeTabId,
  claim,
  newTabAccess,
}: {
  host: BrowserHostSnapshot
  tabs: BrowserTabSnapshot[]
  activeTabId?: string
  claim?: BrowserViewClaimSnapshot
  newTabAccess: AgentAccess
}) {
  const t = useTranslations("Browser")
  const { acceptState, run, busy, detachTab, refresh } = useBrowser()
  const [draggedId, setDraggedId] = useState<string | null>(null)
  const droppedRef = useRef(false)

  const activate = (tabId: string) => {
    if (claim || tabId === activeTabId) return
    void browserApi
      .activateTab(host.hostId, host.generation, tabId)
      .then(acceptState)
      .catch(() => void refresh())
  }

  const moveTab = (
    event: React.DragEvent<HTMLElement>,
    targetIndex: number
  ) => {
    if (claim) return
    const transfer = readTabTransfer(event, draggedId, host.hostId)
    if (!transfer) return
    event.preventDefault()
    event.stopPropagation()
    event.dataTransfer.dropEffect = "move"
    droppedRef.current = true
    void browserApi
      .beginClaim(
        transfer.tabId,
        transfer.sourceHostId,
        host.hostId,
        targetIndex
      )
      .then(() => refresh())
      .catch(() => void refresh())
      .finally(() => setDraggedId(null))
  }

  const finishDrag = (event: React.DragEvent, tabId: string) => {
    const dropped = droppedRef.current
    droppedRef.current = false
    setDraggedId(null)
    if (!dropped && endedOutsideWindow(event)) {
      void detachTab(tabId, host.hostId).catch(() => {})
    }
  }

  return (
    <div className="flex h-9 shrink-0 items-stretch border-b bg-muted/30">
      <div
        className="flex min-w-0 flex-1 overflow-x-auto [scrollbar-width:none] [&::-webkit-scrollbar]:hidden"
        onDragOver={allowDrop}
        onDrop={(event) => moveTab(event, tabs.length)}
      >
        {tabs.map((tab, index) => (
          <div
            key={tab.browserTabId}
            draggable={!claim}
            onDragStart={(event) => {
              droppedRef.current = false
              setDraggedId(tab.browserTabId)
              event.dataTransfer.effectAllowed = "move"
              event.dataTransfer.setData(
                TAB_DRAG_MIME,
                JSON.stringify({
                  tabId: tab.browserTabId,
                  sourceHostId: host.hostId,
                })
              )
              event.dataTransfer.setData(
                "text/plain",
                `${TAB_DRAG_MIME}:${tab.browserTabId}:${host.hostId}`
              )
            }}
            onDragEnd={(event) => finishDrag(event, tab.browserTabId)}
            onDragOver={allowDrop}
            onDrop={(event) => moveTab(event, index)}
            className={cn(
              "group flex h-9 w-48 shrink-0 items-center gap-2 border-r px-2 text-xs",
              tab.browserTabId === activeTabId
                ? "bg-background text-foreground"
                : "text-muted-foreground hover:bg-muted/60"
            )}
          >
            <button
              type="button"
              className="flex min-w-0 flex-1 items-center gap-2 text-left"
              onClick={() => activate(tab.browserTabId)}
            >
              <Globe2 className="size-3.5 shrink-0" />
              <span className="truncate">{tab.title || t("newTab")}</span>
            </button>
            <Button
              variant="ghost"
              size="icon"
              className="size-5 shrink-0 opacity-0 group-hover:opacity-100"
              title={t("closeTab")}
              aria-label={t("closeTab")}
              onClick={() =>
                void run(() => browserApi.closeTab(tab.browserTabId))
              }
            >
              <X className="size-3" />
            </Button>
          </div>
        ))}
      </div>
      <Button
        variant="ghost"
        size="icon"
        className="h-9 w-9 shrink-0 rounded-none"
        title={t("newTab")}
        aria-label={t("newTab")}
        disabled={busy || Boolean(claim)}
        onClick={() =>
          void run(() =>
            browserApi.createTab("about:blank", newTabAccess, host.hostId)
          )
        }
      >
        <Plus className="size-4" />
      </Button>
    </div>
  )
}

function allowDrop(event: React.DragEvent<HTMLElement>) {
  event.preventDefault()
  event.dataTransfer.dropEffect = "move"
}

function readTabTransfer(
  event: React.DragEvent,
  fallbackTabId: string | null,
  fallbackHostId: string
): { tabId: string; sourceHostId: string } | null {
  try {
    const raw = event.dataTransfer.getData(TAB_DRAG_MIME)
    if (raw) {
      const parsed = JSON.parse(raw) as Record<string, unknown>
      if (
        typeof parsed.tabId === "string" &&
        typeof parsed.sourceHostId === "string"
      ) {
        return { tabId: parsed.tabId, sourceHostId: parsed.sourceHostId }
      }
    }
    const text = event.dataTransfer.getData("text/plain")
    const prefix = `${TAB_DRAG_MIME}:`
    if (text.startsWith(prefix)) {
      const [tabId, sourceHostId] = text.slice(prefix.length).split(":", 2)
      if (tabId && sourceHostId) return { tabId, sourceHostId }
    }
  } catch {
    return null
  }
  return fallbackTabId
    ? { tabId: fallbackTabId, sourceHostId: fallbackHostId }
    : null
}

function endedOutsideWindow(event: React.DragEvent): boolean {
  const hasScreenPoint = event.screenX !== 0 || event.screenY !== 0
  const outside =
    event.screenX < window.screenX ||
    event.screenY < window.screenY ||
    event.screenX >= window.screenX + window.outerWidth ||
    event.screenY >= window.screenY + window.outerHeight
  return event.dataTransfer.dropEffect === "none" && hasScreenPoint && outside
}
