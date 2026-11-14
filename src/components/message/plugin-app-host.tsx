"use client"
import { useRef } from "react"
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

export function PluginAppHost(props: PluginAppHostProps) {
  const iframe = useRef<HTMLIFrameElement>(null)
  const download = usePluginAppDownload()
  const { failed, oversized } = usePluginAppHost({
    props,
    iframe,
    download: download.download,
  })
  const t = useTranslations("Folder.chat.contentParts.pluginApp")
  if (oversized) return <OversizedApp />
  if (failed)
    return (
      <div role="alert" className="p-3 text-sm text-muted-foreground">
        {t("unavailable")}
      </div>
    )
  return (
    <>
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
      <PluginAppDownloadDialog {...download} />
    </>
  )
}

function OversizedApp() {
  return (
    <div className="border px-3 py-2 text-xs text-muted-foreground">
      Plugin app content is too large to render.
    </div>
  )
}
