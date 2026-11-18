"use client"

import { useCallback, useEffect, useRef, useState } from "react"
import { Loader2 } from "lucide-react"
import { useTranslations } from "next-intl"
import { PluginAppHost } from "@/components/message/plugin-app-host"
import { Button } from "@/components/ui/button"
import type { PluginPanelTab } from "./use-plugin-panels"
import { usePluginPanelAuthority } from "./use-plugin-panel-authority"
import { usePluginPanelDialog } from "./use-plugin-panel-dialog"
import type { PluginPanelSyncState } from "@/lib/plugin-panel"
import { PanelToolbar, SyncError } from "./plugin-panel-toolbar"
import { StudioFileFrame } from "./studio-file-frame"
import { toast } from "sonner"

const APP_READY_TIMEOUT_MS = 60_000

export function PluginPanelFrame({
  tab,
  visible,
  sync,
  refresh,
  moving,
  onClose,
  onOpenComplete,
  onRetry,
  onDetach,
}: {
  tab: PluginPanelTab
  visible: boolean
  sync?: PluginPanelSyncState
  refresh?: () => void
  moving?: boolean
  onClose?: () => Promise<boolean>
  onOpenComplete?: () => void
  onRetry?: () => Promise<void>
  onDetach?: () => Promise<boolean>
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
  useEffect(() => {
    if (revoked) completeOpening()
  }, [completeOpening, revoked])
  const dialog = usePluginPanelDialog(visible, expanded)
  useAppReadyTimeout(
    visible && !revoked,
    ready,
    failed,
    setFailed,
    completeOpening
  )
  const content = (
    <PanelContent
      visible={visible}
      tab={tab}
      revoked={revoked}
      sync={sync}
      refresh={refresh}
      fullscreen={fullscreen}
      setFullscreen={setFullscreen}
      onClose={onClose}
      ready={ready}
      failed={failed}
      setReady={setReady}
      setFailed={setFailed}
      onOpenComplete={completeOpening}
      onRetry={onRetry}
      onDetach={onDetach}
      moving={moving}
    />
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
  visible,
  tab,
  revoked,
  sync,
  refresh,
  fullscreen,
  setFullscreen,
  onClose,
  ready,
  failed,
  setReady,
  setFailed,
  onOpenComplete,
  onRetry,
  onDetach,
  moving,
}: {
  visible: boolean
  tab: PluginPanelTab
  revoked: boolean | undefined
  sync?: PluginPanelSyncState
  refresh?: () => void
  fullscreen: boolean
  setFullscreen: (value: boolean) => void
  onClose?: () => Promise<boolean>
  ready: boolean
  failed: boolean
  setReady: (value: boolean) => void
  setFailed: (value: boolean) => void
  onOpenComplete?: () => void
  onRetry?: () => Promise<void>
  onDetach?: () => Promise<boolean>
  moving?: boolean
}) {
  const t = useTranslations("PluginPanels")
  return (
    <>
      {sync?.error && <SyncError refresh={refresh} />}
      <PanelToolbar
        label={tab.label}
        fullscreen={fullscreen}
        toggle={() => setFullscreen(!fullscreen)}
        close={
          onClose
            ? async () => {
                if (!window.confirm(t("closeConfirm"))) return
                if (!(await onClose())) window.alert(t("closeFailed"))
              }
            : undefined
        }
        detach={
          onDetach
            ? () => {
                void onDetach().then((success) => {
                  if (!success) toast.error(t("moveFailed"))
                })
              }
            : undefined
        }
        moving={moving}
      />
      <div className="relative min-h-0 flex-1">
        {revoked ? null : tab.kind === "studio-url" ? (
          <StudioFileFrame
            key={tab.id}
            enabled={visible && !moving}
            src={tab.url}
            title={tab.label}
            className="size-full border-0 bg-background"
            sandbox="allow-scripts allow-same-origin allow-forms allow-downloads allow-modals allow-pointer-lock allow-popups"
            allow="clipboard-read; clipboard-write; fullscreen"
            referrerPolicy="no-referrer"
            onLoad={() => {
              setReady(true)
              setFailed(false)
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
              setFailed(false)
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
        {!revoked && !ready && !failed && (
          <div
            className="pointer-events-none absolute inset-0 z-10 flex flex-col items-center justify-center gap-3 bg-background text-sm text-muted-foreground"
            role="status"
            aria-live="polite"
          >
            <Loader2 className="size-5 animate-spin" aria-hidden="true" />
            <span>{t("openingApp", { name: tab.label })}</span>
          </div>
        )}
        {(failed || revoked) && (
          <div
            role="alert"
            className="absolute inset-x-0 bottom-3 mx-auto w-fit rounded-md border bg-background px-3 py-2 text-sm text-destructive shadow-sm"
          >
            {revoked
              ? t("revoked")
              : t(
                  tab.kind === "mcp-app" ? "startupFailed" : "startupTimedOut",
                  { name: tab.label }
                )}
            {onRetry && (
              <Button
                variant="outline"
                size="xs"
                onClick={() => void onRetry()}
              >
                {t("retry")}
              </Button>
            )}
          </div>
        )}
      </div>
    </>
  )
}
