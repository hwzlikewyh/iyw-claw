"use client"

import { useEffect } from "react"
import { Maximize2, Minimize2, RefreshCw } from "lucide-react"
import { useTranslations } from "next-intl"
import { Button } from "@/components/ui/button"
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from "@/components/ui/tooltip"
import { PluginAppHost } from "./plugin-app-host"
import { usePluginAppState } from "./use-plugin-app-state"

type PluginAppContentProps = {
  instanceId: string
  toolCallId: string
  conversationId: number
}

export function PluginAppContent({
  instanceId,
  toolCallId,
  conversationId,
}: PluginAppContentProps) {
  void toolCallId
  const t = useTranslations("Folder.chat.contentParts.pluginApp")
  const { state, fullscreen, setFullscreen, retry } = usePluginAppState(
    instanceId,
    conversationId,
    t("unavailable")
  )

  useEffect(() => {
    if (!fullscreen) return
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") setFullscreen(false)
    }
    window.addEventListener("keydown", onKeyDown)
    return () => window.removeEventListener("keydown", onKeyDown)
  }, [fullscreen, setFullscreen])

  if (state.status === "loading") {
    return (
      <div className="h-[360px] w-full animate-pulse rounded-md border bg-muted/20" />
    )
  }

  if (state.status === "error") {
    return (
      <div className="flex min-h-24 items-center justify-between gap-3 rounded-md border px-3 py-2 text-xs text-muted-foreground">
        <span className="min-w-0">{state.message}</span>
        <Tooltip>
          <TooltipTrigger asChild>
            <Button size="icon-xs" variant="ghost" onClick={() => void retry()}>
              <RefreshCw className="size-3.5" />
              <span className="sr-only">{t("retry")}</span>
            </Button>
          </TooltipTrigger>
          <TooltipContent>{t("retry")}</TooltipContent>
        </Tooltip>
      </div>
    )
  }

  return (
    <div
      data-plugin-app-instance={instanceId}
      data-tool-call-id={toolCallId}
      role={fullscreen ? "dialog" : undefined}
      aria-modal={fullscreen || undefined}
      aria-label={fullscreen ? t("fullscreen") : undefined}
      className={
        fullscreen
          ? "fixed inset-0 z-50 w-full bg-background p-3"
          : "relative w-full overflow-hidden rounded-md border bg-background"
      }
    >
      <PluginAppHost
        html={state.html}
        launch={state.launch}
        hostVersion={state.hostVersion}
        resourceMeta={state.resourceMeta}
        displayMode={fullscreen ? "fullscreen" : "inline"}
        onDisplayModeRequest={(mode) => {
          setFullscreen(mode === "fullscreen")
          return mode
        }}
      />
      <Tooltip>
        <TooltipTrigger asChild>
          <Button
            size="icon-xs"
            variant="secondary"
            className="absolute right-2 top-2"
            onClick={() => setFullscreen((value) => !value)}
          >
            {fullscreen ? (
              <Minimize2 className="size-3.5" />
            ) : (
              <Maximize2 className="size-3.5" />
            )}
            <span className="sr-only">
              {fullscreen ? t("inline") : t("fullscreen")}
            </span>
          </Button>
        </TooltipTrigger>
        <TooltipContent>
          {fullscreen ? t("inline") : t("fullscreen")}
        </TooltipContent>
      </Tooltip>
    </div>
  )
}
