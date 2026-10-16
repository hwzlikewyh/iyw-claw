"use client"

import { useState } from "react"
import {
  Bug,
  Camera,
  Hand,
  MousePointer2,
  PencilLine,
  Share2,
  X,
} from "lucide-react"
import { useTranslations } from "next-intl"
import { toast } from "sonner"
import { useBrowser } from "@/contexts/browser-context"
import { browserApi } from "@/lib/browser-api"
import type { BrowserScreenshot, BrowserTabSnapshot } from "@/lib/browser-types"
import {
  deliverBrowserSession,
  getBrowserSessionTarget,
} from "@/lib/browser-session-handoff"
import { Button } from "@/components/ui/button"
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu"
import { ImagePreviewDialog } from "@/components/ui/image-preview-dialog"

export function BrowserPageActions({
  tab,
  selecting,
  onToggleSelecting,
}: {
  tab: BrowserTabSnapshot | null
  selecting: boolean
  onToggleSelecting: () => void
}) {
  const t = useTranslations("Browser")
  const { state, run, busy } = useBrowser()
  const [capturing, setCapturing] = useState(false)
  const [editing, setEditing] = useState<{
    screenshot: BrowserScreenshot
    target: string
  } | null>(null)
  const held = tab?.controlStatus === "user_held"
  const deliver = async (
    screenshot: BrowserScreenshot,
    target: string,
    name: string
  ) => {
    if (
      !(await deliverBrowserSession(target, {
        kind: "image",
        image: { ...screenshot, name },
      }))
    ) {
      throw new Error(t("conversationUnavailable"))
    }
    toast.success(t("screenshotAdded"))
  }
  const capture = async (annotate: boolean) => {
    if (!tab || capturing) return
    setCapturing(true)
    try {
      const target = await getBrowserSessionTarget()
      const screenshot = await browserApi.captureScreenshot(tab.browserTabId)
      if (annotate) setEditing({ screenshot, target })
      else await deliver(screenshot, target, "browser-screenshot.png")
    } catch (error) {
      console.warn("[Browser] screenshot handoff failed", { error })
      toast.error(t("screenshotFailed"))
    } finally {
      setCapturing(false)
    }
  }
  const share = async () => {
    if (!tab) return
    try {
      const target = await getBrowserSessionTarget()
      const accepted = await deliverBrowserSession(target, {
        kind: "text",
        text: `${tab.title.trim() || tab.url}\n${tab.url}`,
      })
      if (!accepted) throw new Error("The conversation input is unavailable")
      toast.success(t("pageAdded"))
    } catch (error) {
      console.warn("[Browser] page handoff failed", { error })
      toast.error(t("conversationUnavailable"))
    }
  }
  return (
    <>
      {selecting ? (
        <Button
          variant="ghost"
          size="sm"
          className="h-7 gap-1 bg-violet-500/10 px-2 text-xs text-violet-600"
          onClick={onToggleSelecting}
          aria-label={t("cancelSelection")}
        >
          <X className="size-3.5" />
          {t("selecting")}
        </Button>
      ) : (
        <DropdownMenu>
          <DropdownMenuTrigger asChild>
            <Button
              variant="ghost"
              size="icon"
              className="size-7 shrink-0"
              disabled={!tab || busy || capturing}
              title={t("sendPage")}
              aria-label={t("sendPage")}
            >
              <MousePointer2 className="size-3.5" />
            </Button>
          </DropdownMenuTrigger>
          <DropdownMenuContent align="end" className="w-60">
            <DropdownMenuLabel className="text-xs font-normal text-muted-foreground">
              {t("sendPage")}
            </DropdownMenuLabel>
            <DropdownMenuItem onSelect={onToggleSelecting}>
              <MousePointer2 />
              {t("selectElement")}
            </DropdownMenuItem>
            <DropdownMenuItem onSelect={() => void capture(false)}>
              <Camera />
              {t("sendScreenshot")}
            </DropdownMenuItem>
            <DropdownMenuItem onSelect={() => void capture(true)}>
              <PencilLine />
              {t("annotateScreenshot")}
            </DropdownMenuItem>
            <DropdownMenuItem onSelect={() => void share()}>
              <Share2 />
              {t("sendPage")}
            </DropdownMenuItem>
            <DropdownMenuSeparator />
            <DropdownMenuItem disabled>
              <Bug />
              {tab?.pageErrorCount
                ? t("pageErrors", { count: tab.pageErrorCount })
                : t("pageHealthy")}
            </DropdownMenuItem>
            <DropdownMenuItem
              onSelect={() =>
                tab &&
                void run(() => browserApi.setUserHeld(tab.browserTabId, !held))
              }
            >
              <Hand />
              {held ? t("releaseControl") : t("holdControl")}
            </DropdownMenuItem>
            <DropdownMenuLabel className="text-xs font-normal text-muted-foreground">
              {state?.runtime.iywLoginStatus === "authenticated"
                ? t("iywLogin.authenticated")
                : t("iywLogin.unauthenticated")}
            </DropdownMenuLabel>
          </DropdownMenuContent>
        </DropdownMenu>
      )}
      {editing ? (
        <ImagePreviewDialog
          src={`data:${editing.screenshot.mimeType};base64,${editing.screenshot.data}`}
          alt="browser-screenshot.png"
          open
          initialMode="annotate"
          onOpenChange={(open) => {
            if (!open) setEditing(null)
          }}
          onSendToChat={(result) =>
            deliver(
              { data: result.data, mimeType: result.mime_type },
              editing.target,
              result.name
            )
          }
        />
      ) : null}
    </>
  )
}
