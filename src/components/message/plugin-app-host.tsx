"use client"
import { useRef, useState } from "react"
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
  onRetry?: () => Promise<void>
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
  const panels = useTranslations("PluginPanels")
  if (oversized) return <OversizedApp />
  if (failed)
    return (
      <FailedApp
        message={t("unavailable")}
        label={panels("retryOpen")}
        retry={props.onRetry}
      />
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

function FailedApp({
  message,
  label,
  retry,
}: {
  message: string
  label: string
  retry?: () => Promise<void>
}) {
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState("")
  const run = async () => {
    if (!retry || busy) return
    setBusy(true)
    setError("")
    try {
      await retry()
    } catch (cause) {
      setError(String(cause))
    } finally {
      setBusy(false)
    }
  }
  return (
    <div role="alert" className="p-3 text-sm text-muted-foreground">
      <p>{message}</p>
      {error && <p className="mt-2 break-words text-destructive">{error}</p>}
      {retry && (
        <Button
          variant="outline"
          disabled={busy}
          className="mt-3 block"
          onClick={() => void run()}
        >
          {label}
        </Button>
      )}
    </div>
  )
}

function OversizedApp() {
  return (
    <div className="border px-3 py-2 text-xs text-muted-foreground">
      Plugin app content is too large to render.
    </div>
  )
}
