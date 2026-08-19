"use client"

import { useCallback, useEffect, useState } from "react"
import { useTranslations } from "next-intl"

import { Button } from "@/components/ui/button"
import { Checkbox } from "@/components/ui/checkbox"
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"
import { toErrorMessage } from "@/lib/app-error"
import { isDesktop } from "@/lib/platform"
import {
  cancelMainClose,
  completeMainClose,
  getPendingMainCloseRequest,
  MAIN_CLOSE_REQUESTED_EVENT,
  type CloseAction,
  type MainCloseRequestPayload,
} from "@/lib/tauri"

function cancelPendingRequest() {
  void cancelMainClose().catch((error) => {
    console.warn("[MainCloseDialog] failed to cancel close request:", error)
  })
}

async function listenForMainClose(
  onRequest: (payload: MainCloseRequestPayload) => void
) {
  const [{ listen }, { getCurrentWebviewWindow }] = await Promise.all([
    import("@tauri-apps/api/event"),
    import("@tauri-apps/api/webviewWindow"),
  ])
  if (getCurrentWebviewWindow().label !== "main") return null
  const unlisten = await listen<MainCloseRequestPayload>(
    MAIN_CLOSE_REQUESTED_EVENT,
    (event) => onRequest(event.payload)
  )
  try {
    const pending = await getPendingMainCloseRequest()
    if (pending) onRequest(pending)
  } catch (error) {
    console.warn("[MainCloseDialog] pending close lookup failed:", error)
  }
  return unlisten
}

function useCloseRequestListener(
  onRequest: (payload: MainCloseRequestPayload) => void
) {
  useEffect(() => {
    if (!isDesktop()) return
    let disposed = false
    let unlisten: (() => void) | null = null

    void listenForMainClose(onRequest)
      .then((cleanup) => {
        if (!cleanup) return
        if (disposed) {
          cleanup()
          cancelPendingRequest()
        } else {
          unlisten = cleanup
        }
      })
      .catch((error) => {
        console.error("[MainCloseDialog] close listener failed:", error)
      })

    return () => {
      disposed = true
      if (unlisten) {
        unlisten()
        cancelPendingRequest()
      }
    }
  }, [onRequest])
}

function useMainCloseDialog() {
  const [request, setRequest] = useState<MainCloseRequestPayload | null>(null)
  const [remember, setRemember] = useState(false)
  const [busy, setBusy] = useState<CloseAction | "cancel" | null>(null)
  const [error, setError] = useState<string | null>(null)
  const onRequest = useCallback((payload: MainCloseRequestPayload) => {
    setRemember(false)
    setError(null)
    setRequest(payload)
  }, [])
  useCloseRequestListener(onRequest)

  const cancel = useCallback(async () => {
    setBusy("cancel")
    setError(null)
    try {
      await cancelMainClose()
      setRequest(null)
      setRemember(false)
    } catch (cause) {
      setError(toErrorMessage(cause))
    } finally {
      setBusy(null)
    }
  }, [])

  const act = useCallback(
    async (action: CloseAction) => {
      setBusy(action)
      setError(null)
      try {
        await completeMainClose(action, remember)
        setRequest(null)
        setRemember(false)
      } catch (cause) {
        setError(toErrorMessage(cause))
      } finally {
        setBusy(null)
      }
    },
    [remember]
  )

  return { request, remember, setRemember, busy, error, cancel, act }
}

function CloseDialogActions({
  canHideToTray,
  busy,
  onCancel,
  onAction,
}: {
  canHideToTray: boolean
  busy: CloseAction | "cancel" | null
  onCancel: () => Promise<void>
  onAction: (action: CloseAction) => Promise<void>
}) {
  const t = useTranslations("MainCloseDialog")
  return (
    <DialogFooter>
      <Button
        variant="outline"
        onClick={() => void onCancel()}
        disabled={busy !== null}
      >
        {t("cancel")}
      </Button>
      <Button
        variant="secondary"
        onClick={() => void onAction("exit")}
        disabled={busy !== null}
      >
        {t("exit")}
      </Button>
      {canHideToTray && (
        <Button onClick={() => void onAction("tray")} disabled={busy !== null}>
          {t("tray")}
        </Button>
      )}
    </DialogFooter>
  )
}

export function MainCloseDialog() {
  const t = useTranslations("MainCloseDialog")
  const dialog = useMainCloseDialog()
  return (
    <Dialog
      open={dialog.request !== null}
      onOpenChange={(open) => {
        if (!open && dialog.busy === null) void dialog.cancel()
      }}
    >
      <DialogContent
        className="sm:max-w-md"
        onPointerDownOutside={(event) => event.preventDefault()}
      >
        <DialogHeader>
          <DialogTitle>{t("title")}</DialogTitle>
          <DialogDescription>{t("description")}</DialogDescription>
        </DialogHeader>
        <label className="flex items-center gap-2 text-sm">
          <Checkbox
            checked={dialog.remember}
            onCheckedChange={(checked) => dialog.setRemember(checked === true)}
            disabled={dialog.busy !== null}
          />
          {t("remember")}
        </label>
        {dialog.error && (
          <p className="text-xs text-destructive">
            {t("actionFailed", { message: dialog.error })}
          </p>
        )}
        <CloseDialogActions
          canHideToTray={dialog.request?.canHideToTray ?? false}
          busy={dialog.busy}
          onCancel={dialog.cancel}
          onAction={dialog.act}
        />
      </DialogContent>
    </Dialog>
  )
}
