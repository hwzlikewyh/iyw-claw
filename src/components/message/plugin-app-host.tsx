"use client"
import { useCallback, useEffect, useRef, useState } from "react"
import { useTranslations } from "next-intl"
import type { PluginAppResourceMeta } from "@/lib/api"
import {
  buildPluginAppProxyDocument,
  type PluginAppLaunch,
} from "@/lib/plugin-app-bridge"
import type { PluginPanelUpdate } from "@/lib/plugin-panel"
import { usePluginAppHost } from "./use-plugin-app-host"
import { usePluginAppDownload } from "./use-plugin-app-download"
import { PluginAppDownloadDialog } from "./plugin-app-download-dialog"
import { Button } from "@/components/ui/button"
export type { MessageContext } from "./plugin-app-messaging"
type DisplayMode = "inline" | "fullscreen"
export type PluginAppHostProps = {
  html: string
  launch: PluginAppLaunch
  hostVersion: string
  resourceMeta?: PluginAppResourceMeta
  displayMode?: DisplayMode
  onDisplayModeRequest?: (mode: DisplayMode) => DisplayMode
  fill?: boolean
  toolUpdate?: PluginPanelUpdate
  onReady?: () => void
  onFailure?: () => void
}

const FAILURE_DETAIL_LIMIT = 1200

export function PluginAppHost(props: PluginAppHostProps) {
  const iframe = useRef<HTMLIFrameElement>(null)
  const download = usePluginAppDownload()
  const { failed, oversized } = usePluginAppHost({
    props,
    iframe,
    download: download.download,
  })
  const t = useTranslations("Folder.chat.contentParts.pluginApp")
  const [detail, setDetail] = useState<string | null>(null)
  useEffect(() => {
    setDetail(null)
  }, [props.launch.instanceId])
  useEffect(() => {
    const timer = window.setInterval(() => {
      const value = readFailure(iframe.current?.contentWindow)
      if (value) setDetail(value)
    }, 500)
    return () => window.clearInterval(timer)
  }, [])
  const refresh = useCallback(() => {
    setDetail(null)
    props.onFailure?.()
  }, [props])
  if (oversized) return <OversizedApp />
  return (
    <>
      <div className={props.fill ? "relative h-full min-h-0" : "relative"}>
        {failed && (
          <div
            role="alert"
            className="absolute inset-x-0 top-0 z-10 bg-background p-3 text-sm text-muted-foreground"
          >
            <p>{t("unavailable")}</p>
            {detail && (
              <pre className="mt-2 max-h-40 overflow-auto whitespace-pre-wrap break-words rounded border bg-muted/40 p-2 font-mono text-xs">
                {detail}
              </pre>
            )}
            <div className="mt-2 flex gap-2">
              <Button size="xs" variant="outline" onClick={refresh}>
                {t("retry")}
              </Button>
              <Button
                size="xs"
                variant="ghost"
                onClick={() => {
                  if (detail) void navigator.clipboard?.writeText(detail)
                }}
                disabled={!detail}
              >
                {t("copyReason")}
              </Button>
            </div>
          </div>
        )}
        <iframe
          ref={iframe}
          title="Plugin app"
          data-fit={props.fill ? "panel" : undefined}
          sandbox="allow-scripts"
          srcDoc={buildPluginAppProxyDocument(props.resourceMeta)}
          className={
            props.fill
              ? "h-full w-full border-0"
              : props.displayMode === "fullscreen"
                ? "h-[calc(100dvh-3rem)] w-full border-0"
                : "h-[360px] w-full border-0"
          }
        />
      </div>
      <PluginAppDownloadDialog {...download} />
    </>
  )
}

// 面板运行在 sandbox 内，异常不会冒泡到宿主；代理文档把失败原因转发到本窗口消息里。
function readFailure(scope: Window | null | undefined): string | null {
  if (!scope) return null
  try {
    const report = (
      scope as Window & {
        __iywPluginAppFailure?: () => string | null | undefined
      }
    ).__iywPluginAppFailure?.()
    const message = report?.trim()
    return message ? message.slice(0, FAILURE_DETAIL_LIMIT) : null
  } catch {
    return null
  }
}
function OversizedApp() {
  return (
    <div className="border px-3 py-2 text-xs text-muted-foreground">
      Plugin app content is too large to render.
    </div>
  )
}
