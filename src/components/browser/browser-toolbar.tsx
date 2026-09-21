"use client"

import { useState } from "react"
import {
  ArrowLeft,
  ArrowRight,
  Camera,
  CircleAlert,
  CircleCheck,
  Hand,
  MousePointer2,
  PanelTopClose,
  RotateCw,
  Share2,
  SquareArrowOutUpRight,
  X,
} from "lucide-react"
import { useTranslations } from "next-intl"
import { toast } from "sonner"
import { useBrowser } from "@/contexts/browser-context"
import { browserApi } from "@/lib/browser-api"
import {
  emitAppendTextToSession,
  emitAttachImageToSession,
} from "@/lib/session-attachment-events"
import { useTabStore } from "@/stores/tab-store"
import type {
  BrowserHostSnapshot,
  BrowserTabSnapshot,
} from "@/lib/browser-types"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { BrowserDownloads } from "./browser-downloads"

export function BrowserToolbar({
  host,
  tab,
  selecting,
  onToggleSelecting,
  onClose,
}: {
  host: BrowserHostSnapshot
  tab: BrowserTabSnapshot | null
  selecting: boolean
  onToggleSelecting: () => void
  onClose?: () => void
}) {
  const t = useTranslations("Browser")
  const { state, run, busy, detachTab, refresh } = useBrowser()
  const [address, setAddress] = useState(tab?.url ?? "")
  const [capturing, setCapturing] = useState(false)
  const sessionTabId = useTabStore((store) => store.activeTabId)

  const navigate = () => {
    if (!tab) return
    const url = normalizeAddress(address)
    setAddress(url)
    void run(() => browserApi.navigate(tab.browserTabId, url))
  }

  const dock = () => {
    if (!tab) return
    const main = state?.hosts.find((item) => item.windowLabel === "main")
    if (!main) return
    void browserApi
      .beginClaim(
        tab.browserTabId,
        host.hostId,
        main.hostId,
        main.tabOrder.length
      )
      .then(() => refresh())
      .catch(() => void refresh())
  }

  const held = tab?.controlStatus === "user_held"

  const sharePage = () => {
    if (!tab || !sessionTabId) return
    const title = tab.title.trim() || tab.url
    emitAppendTextToSession({
      tabId: sessionTabId,
      text: `${title}\n${tab.url}`,
    })
    toast.success(t("pageAdded"))
  }

  const sendScreenshot = async () => {
    if (!tab || !sessionTabId || capturing) return
    setCapturing(true)
    try {
      const screenshot = await browserApi.captureScreenshot(tab.browserTabId)
      emitAttachImageToSession({
        tabId: sessionTabId,
        data: screenshot.data,
        mimeType: screenshot.mimeType,
        name: "browser-screenshot.png",
      })
      toast.success(t("screenshotAdded"))
    } catch {
      toast.error(t("screenshotFailed"))
    } finally {
      setCapturing(false)
    }
  }

  return (
    <div className="flex h-10 shrink-0 items-center gap-1 border-b px-2">
      <ToolButton
        label={t("back")}
        icon={ArrowLeft}
        disabled={!tab || busy}
        onClick={() => tab && void run(() => browserApi.back(tab.browserTabId))}
      />
      <ToolButton
        label={t("forward")}
        icon={ArrowRight}
        disabled={!tab || busy}
        onClick={() =>
          tab && void run(() => browserApi.forward(tab.browserTabId))
        }
      />
      <ToolButton
        label={t("reload")}
        icon={RotateCw}
        disabled={!tab || busy}
        onClick={() =>
          tab && void run(() => browserApi.reload(tab.browserTabId))
        }
      />
      <Input
        value={address}
        onChange={(event) => setAddress(event.target.value)}
        onKeyDown={(event) => {
          if (event.key === "Enter") navigate()
          if (event.key === "Escape") setAddress(tab?.url ?? "")
        }}
        className="h-7 min-w-0 flex-1 rounded-md bg-muted/50 px-3 text-xs"
        aria-label={t("address")}
        spellCheck={false}
      />
      <ToolButton
        label={t("sendPage")}
        icon={Share2}
        disabled={!tab || !sessionTabId}
        onClick={sharePage}
      />
      <ToolButton
        label={t("sendScreenshot")}
        icon={Camera}
        disabled={!tab || !sessionTabId || busy || capturing}
        onClick={() => void sendScreenshot()}
      />
      <ToolButton
        label={t("selectElement")}
        icon={MousePointer2}
        active={selecting}
        disabled={!tab || !sessionTabId || busy || capturing}
        onClick={onToggleSelecting}
      />
      <PageErrorIndicator errors={tab?.pageErrorCount ?? 0} />
      {state?.runtime.iywLoginStatus === "authenticated" ? (
        <span
          className="flex size-7 shrink-0 items-center justify-center text-emerald-600"
          title={t("iywLogin.authenticated")}
          aria-label={t("iywLogin.authenticated")}
          role="img"
        >
          <CircleCheck className="size-3.5" />
        </span>
      ) : state?.runtime.iywLoginStatus === "unauthenticated" ? (
        <span
          className="flex size-7 shrink-0 items-center justify-center text-muted-foreground"
          title={t("iywLogin.unauthenticated")}
          aria-label={t("iywLogin.unauthenticated")}
          role="img"
        >
          <CircleAlert className="size-3.5" />
        </span>
      ) : null}
      <ToolButton
        label={held ? t("releaseControl") : t("holdControl")}
        icon={Hand}
        active={held}
        disabled={!tab}
        onClick={() =>
          tab && void run(() => browserApi.setUserHeld(tab.browserTabId, !held))
        }
      />
      {host.kind === "docked" ? (
        <ToolButton
          label={t("detachTab")}
          icon={SquareArrowOutUpRight}
          disabled={!tab || busy}
          onClick={() =>
            tab && void detachTab(tab.browserTabId, host.hostId).catch(() => {})
          }
        />
      ) : (
        <ToolButton
          label={t("dockTab")}
          icon={PanelTopClose}
          disabled={
            !tab || !state?.hosts.some((item) => item.windowLabel === "main")
          }
          onClick={dock}
        />
      )}
      <BrowserDownloads />
      {onClose ? (
        <ToolButton label={t("closeBrowser")} icon={X} onClick={onClose} />
      ) : null}
    </div>
  )
}

function PageErrorIndicator({ errors }: { errors: number }) {
  const t = useTranslations("Browser")
  const Icon = errors > 0 ? CircleAlert : CircleCheck
  const label =
    errors > 0 ? t("pageErrors", { count: errors }) : t("pageHealthy")
  return (
    <span
      className={`flex size-7 shrink-0 items-center justify-center ${
        errors > 0 ? "text-amber-600" : "text-emerald-600"
      }`}
      title={label}
      aria-label={label}
      role="img"
    >
      <Icon className="size-3.5" />
    </span>
  )
}

function ToolButton({
  label,
  icon: Icon,
  onClick,
  disabled = false,
  active = false,
}: {
  label: string
  icon: typeof ArrowLeft
  onClick: () => void
  disabled?: boolean
  active?: boolean
}) {
  return (
    <Button
      variant="ghost"
      size="icon"
      className={`size-7 shrink-0 ${active ? "bg-accent" : ""}`}
      title={label}
      aria-label={label}
      disabled={disabled}
      onClick={onClick}
    >
      <Icon className="size-3.5" />
    </Button>
  )
}

function normalizeAddress(value: string): string {
  const trimmed = value.trim()
  if (!trimmed) return "about:blank"
  if (/^https?:\/\//i.test(trimmed) || trimmed === "about:blank") {
    return trimmed
  }
  if (/^(localhost|127\.0\.0\.1|\[::1\])(?::\d+)?(?:\/|$)/i.test(trimmed)) {
    return `http://${trimmed}`
  }
  if (/^[\p{L}\p{N}-]+(?:\.[\p{L}\p{N}-]+)+(?::\d+)?(?:\/|$)/u.test(trimmed)) {
    return `https://${trimmed}`
  }
  return `https://www.google.com/search?q=${encodeURIComponent(trimmed)}`
}
