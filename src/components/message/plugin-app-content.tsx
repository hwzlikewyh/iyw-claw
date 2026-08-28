"use client"

import { useCallback, useEffect, useState } from "react"
import { Maximize2, Minimize2, RefreshCw } from "lucide-react"
import { useTranslations } from "next-intl"
import {
  pluginAppOpen,
  type PluginAppLaunch,
  type PluginAppResourceMeta,
} from "@/lib/api"
import { Button } from "@/components/ui/button"
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from "@/components/ui/tooltip"
import { PluginAppHost } from "./plugin-app-host"

type PluginAppContentProps = {
  instanceId: string
  toolCallId: string
  conversationId: number
}

type AppState =
  | { status: "loading" }
  | {
      status: "ready"
      html: string
      launch: PluginAppLaunch
      hostVersion: string
      resourceMeta?: PluginAppResourceMeta
    }
  | { status: "error"; message: string }

export function PluginAppContent({
  instanceId,
  toolCallId,
  conversationId,
}: PluginAppContentProps) {
  void toolCallId
  const t = useTranslations("Folder.chat.contentParts.pluginApp")
  const [state, setState] = useState<AppState>({ status: "loading" })
  const [fullscreen, setFullscreen] = useState(false)

  const openApp = useCallback(async () => {
    return pluginAppOpen({ instanceId, conversationId })
  }, [conversationId, instanceId])

  const retry = useCallback(async () => {
    setState({ status: "loading" })
    try {
      const response = await openApp()
      setFullscreen(response.launch.displayMode === "fullscreen")
      setState({
        status: "ready",
        html: response.html,
        launch: response.launch,
        hostVersion: response.hostVersion,
        resourceMeta: response.resourceMeta,
      })
    } catch (error) {
      setState({
        status: "error",
        message:
          error instanceof Error ? error.message : "Plugin app unavailable",
      })
    }
  }, [openApp])

  useEffect(() => {
    let active = true
    void openApp().then(
      (response) => {
        if (active) {
          setFullscreen(response.launch.displayMode === "fullscreen")
          setState({
            status: "ready",
            html: response.html,
            launch: response.launch,
            hostVersion: response.hostVersion,
            resourceMeta: response.resourceMeta,
          })
        }
      },
      (error: unknown) => {
        if (active) {
          setState({
            status: "error",
            message:
              error instanceof Error ? error.message : "Plugin app unavailable",
          })
        }
      }
    )
    return () => {
      active = false
    }
  }, [openApp])

  useEffect(() => {
    if (!fullscreen) return
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") setFullscreen(false)
    }
    window.addEventListener("keydown", onKeyDown)
    return () => window.removeEventListener("keydown", onKeyDown)
  }, [fullscreen])

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
