"use client"

import { useCallback, useEffect, useRef, useState } from "react"
import { Loader2, Maximize2, Minimize2, X } from "lucide-react"
import { useTranslations } from "next-intl"
import { PluginAppHost } from "@/components/message/plugin-app-host"
import { Button } from "@/components/ui/button"
import type { PluginPanelTab } from "./use-plugin-panels"
import { usePluginPanelAuthority } from "./use-plugin-panel-authority"
import { usePluginPanelDialog } from "./use-plugin-panel-dialog"
import type { PluginPanelSyncState } from "@/lib/plugin-panel"

const APP_READY_TIMEOUT_MS = 60_000

export function PluginPanelFrame({
  tab,
  visible,
  sync,
  refresh,
  moving,
  mode = "panel",
  onClose,
  onOpenComplete,
}: {
  tab: PluginPanelTab
  visible: boolean
  sync?: PluginPanelSyncState
  refresh?: () => void
  moving?: boolean
  mode?: "panel" | "main"
  onClose?: () => Promise<boolean>
  onOpenComplete?: () => void
}) {
  const [fullscreen, setFullscreen] = useState(false)
  const authorityLost = usePluginPanelAuthority(tab)
  const revoked = authorityLost || sync?.forbidden
  const expanded = fullscreen && !revoked
  const [ready, setReady] = useState(false)
  const [failed, setFailed] = useState(false)
  const onOpenCompleteRef = useRef(onOpenComplete)
  useEffect(() => {
    onOpenCompleteRef.current = onOpenComplete
  }, [onOpenComplete])
  const completeOpening = useCallback(() => onOpenCompleteRef.current?.(), [])
  const dialog = usePluginPanelDialog(visible && mode === "panel", expanded)
  useAppReadyTimeout(
    visible && !revoked,
    ready,
    failed,
    setFailed,
    completeOpening
  )
  const content = (
    <PanelContent
      tab={tab}
      revoked={revoked}
      sync={sync}
      refresh={refresh}
      fullscreen={fullscreen}
      setFullscreen={setFullscreen}
      main={mode === "main"}
      onClose={onClose}
      ready={ready}
      failed={failed}
      setReady={setReady}
      setFailed={setFailed}
      onOpenComplete={completeOpening}
    />
  )
  if (mode === "main")
    return (
      <section
        aria-label={tab.label}
        inert={!visible || moving || undefined}
        className={
          expanded
            ? "fixed inset-0 z-[80] flex h-dvh w-screen flex-col bg-background text-foreground"
            : "pointer-events-auto absolute inset-0 z-[80] flex h-full w-full flex-col bg-background text-foreground"
        }
        style={{ display: visible ? "flex" : "none" }}
      >
        {content}
      </section>
    )
  return (
    <dialog
      ref={dialog}
      aria-label={tab.label}
      inert={moving || undefined}
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
      {content}
    </dialog>
  )
}

function useAppReadyTimeout(
  visible: boolean,
  ready: boolean,
  failed: boolean,
  setFailed: (value: boolean) => void,
  onTimeout: () => void
) {
  useEffect(() => {
    if (!visible || ready || failed) return
    const timer = window.setTimeout(() => {
      setFailed(true)
      onTimeout()
    }, APP_READY_TIMEOUT_MS)
    return () => window.clearTimeout(timer)
  }, [failed, onTimeout, ready, setFailed, visible])
}

function PanelContent({
  tab,
  revoked,
  sync,
  refresh,
  fullscreen,
  setFullscreen,
  main,
  onClose,
  ready,
  failed,
  setReady,
  setFailed,
  onOpenComplete,
}: {
  tab: PluginPanelTab
  revoked: boolean | undefined
  sync?: PluginPanelSyncState
  refresh?: () => void
  fullscreen: boolean
  setFullscreen: (value: boolean) => void
  main: boolean
  onClose?: () => Promise<boolean>
  ready: boolean
  failed: boolean
  setReady: (value: boolean) => void
  setFailed: (value: boolean) => void
  onOpenComplete?: () => void
}) {
  const t = useTranslations("PluginPanels")
  if (revoked)
    return (
      <p role="alert" className="p-4 text-sm text-muted-foreground">
        {t("revoked")}
      </p>
    )
  return (
    <>
      {sync?.error && <SyncError refresh={refresh} />}
      <PanelToolbar
        label={tab.label}
        fullscreen={fullscreen}
        toggle={() => setFullscreen(!fullscreen)}
        close={
          main && onClose
            ? async () => {
                if (!window.confirm(t("closeConfirm"))) return
                if (!(await onClose())) window.alert(t("closeFailed"))
              }
            : undefined
        }
      />
      <div className="relative min-h-0 flex-1">
        {tab.kind === "studio-url" ? (
          <iframe
            src={tab.url}
            title={tab.label}
            className="size-full border-0 bg-background"
            sandbox="allow-scripts allow-same-origin allow-forms allow-downloads allow-modals allow-pointer-lock allow-popups"
            allow="clipboard-read; clipboard-write; fullscreen"
            referrerPolicy="no-referrer"
            onLoad={() => {
              setReady(true)
              onOpenComplete?.()
            }}
            onError={() => {
              setFailed(true)
              onOpenComplete?.()
            }}
          />
        ) : (
          <PluginAppHost
            {...tab.response}
            fill
            toolUpdate={sync?.update}
            onReady={() => {
              setReady(true)
              onOpenComplete?.()
            }}
            onFailure={() => {
              setFailed(true)
              onOpenComplete?.()
            }}
            displayMode={fullscreen ? "fullscreen" : "inline"}
            onDisplayModeRequest={(mode) => {
              setFullscreen(mode === "fullscreen")
              return mode
            }}
          />
        )}
        {!ready && !failed && (
          <div
            className="pointer-events-none absolute inset-0 z-10 flex flex-col items-center justify-center gap-3 bg-background text-sm text-muted-foreground"
            role="status"
            aria-live="polite"
          >
            <Loader2 className="size-5 animate-spin" aria-hidden="true" />
            <span>{t("openingApp", { name: tab.label })}</span>
          </div>
        )}
        {failed && (
          <p
            role="alert"
            className="absolute inset-x-0 bottom-3 mx-auto w-fit rounded-md border bg-background px-3 py-2 text-sm text-destructive shadow-sm"
          >
            {t(tab.kind === "mcp-app" ? "startupFailed" : "startupTimedOut", {
              name: tab.label,
            })}
          </p>
        )}
      </div>
    </>
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
  close,
}: {
  label: string
  fullscreen: boolean
  toggle: () => void
  close?: () => void
}) {
  const t = useTranslations("PluginPanels")
  return (
    <div className="flex h-8 shrink-0 items-center justify-between border-b px-2 max-md:h-11">
      <span className="truncate text-xs">{label}</span>
      <div className="flex shrink-0 items-center gap-1">
        {close && (
          <Button
            variant="ghost"
            size="icon-xs"
            aria-label={t("closeApp", { name: label })}
            onClick={close}
          >
            <X />
          </Button>
        )}
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
    </div>
  )
}
