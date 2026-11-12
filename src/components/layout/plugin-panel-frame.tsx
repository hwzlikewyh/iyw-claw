"use client"

import { useState } from "react"
import { Maximize2, Minimize2 } from "lucide-react"
import { useTranslations } from "next-intl"
import { PluginAppHost } from "@/components/message/plugin-app-host"
import { Button } from "@/components/ui/button"
import type { PluginPanelTab } from "./use-plugin-panels"
import { usePluginPanelAuthority } from "./use-plugin-panel-authority"
import { usePluginPanelDialog } from "./use-plugin-panel-dialog"
import type { PluginPanelSyncState } from "@/lib/plugin-panel"

export function PluginPanelFrame({
  tab,
  visible,
  sync,
  refresh,
}: {
  tab: PluginPanelTab
  visible: boolean
  sync?: PluginPanelSyncState
  refresh?: () => void
}) {
  const t = useTranslations("PluginPanels"),
    [fullscreen, setFullscreen] = useState(false)
  const authorityLost = usePluginPanelAuthority(tab)
  const revoked = authorityLost || sync?.forbidden
  const expanded = fullscreen && !revoked
  const dialog = usePluginPanelDialog(visible, expanded)
  return (
    <dialog
      ref={dialog}
      aria-label={tab.label}
      onCancel={(event) => {
        event.preventDefault()
        event.stopPropagation()
        setFullscreen(false)
      }}
      className={
        expanded && visible
          ? "fixed inset-0 m-0 h-dvh max-h-none w-screen max-w-none flex-col border-0 bg-background p-0 text-foreground"
          : "relative inset-auto m-0 h-full min-h-0 w-full max-w-none flex-1 flex-col border-0 bg-background p-0 text-foreground"
      }
      style={{ display: visible ? "flex" : "none" }}
    >
      {revoked ? (
        <p role="alert" className="p-4 text-sm text-muted-foreground">
          {t("revoked")}
        </p>
      ) : (
        <>
          {sync?.error && <SyncError refresh={refresh} />}
          <PanelToolbar
            label={tab.label}
            fullscreen={fullscreen}
            toggle={() => setFullscreen((value) => !value)}
          />
          <div className="min-h-0 flex-1">
            <PluginAppHost
              {...tab.response}
              fill
              toolUpdate={sync?.update}
              displayMode={fullscreen ? "fullscreen" : "inline"}
              onDisplayModeRequest={(mode) => {
                setFullscreen(mode === "fullscreen")
                return mode
              }}
            />
          </div>
        </>
      )}
    </dialog>
  )
}

function SyncError({ refresh }: { refresh?: () => void }) {
  const t = useTranslations("PluginPanels")
  return (
    <div
      role="status"
      className="flex items-center justify-between gap-2 px-3 py-2 text-xs text-muted-foreground"
    >
      <span>{t("syncFailed")}</span>
      <Button size="xs" variant="link" onClick={refresh}>
        {t("refresh")}
      </Button>
    </div>
  )
}

function PanelToolbar({
  label,
  fullscreen,
  toggle,
}: {
  label: string
  fullscreen: boolean
  toggle: () => void
}) {
  const t = useTranslations("PluginPanels")
  return (
    <div className="flex h-8 shrink-0 items-center justify-between border-b px-2 max-md:h-11">
      <span className="truncate text-xs">{label}</span>
      <Button
        variant="ghost"
        size="icon-xs"
        className="max-md:size-11"
        aria-label={t(fullscreen ? "restore" : "maximize")}
        onClick={toggle}
      >
        {fullscreen ? <Minimize2 /> : <Maximize2 />}
      </Button>
    </div>
  )
}
