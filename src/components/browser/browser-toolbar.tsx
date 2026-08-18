"use client"

import { useState } from "react"
import {
  ArrowLeft,
  ArrowRight,
  Hand,
  PanelTopClose,
  RotateCw,
  SquareArrowOutUpRight,
  X,
} from "lucide-react"
import { useTranslations } from "next-intl"
import { useBrowser } from "@/contexts/browser-context"
import { browserApi } from "@/lib/browser-api"
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
  onClose,
}: {
  host: BrowserHostSnapshot
  tab: BrowserTabSnapshot | null
  onClose?: () => void
}) {
  const t = useTranslations("Browser")
  const { state, run, busy, detachTab, refresh } = useBrowser()
  const [address, setAddress] = useState(tab?.url ?? "")

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
