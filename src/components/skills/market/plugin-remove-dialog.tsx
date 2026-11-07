"use client"

import { useEffect, useState } from "react"
import { useTranslations } from "next-intl"
import { useIywAccount } from "@/contexts/iyw-account-context"
import { onTransportChange } from "@/lib/transport"
import { Button } from "@/components/ui/button"
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogDescription,
  DialogFooter,
} from "@/components/ui/dialog"
import { usePluginRemove } from "./use-plugin-remove"
import { PluginRemoveDetails } from "./plugin-remove-details"

interface Props {
  id: string
  history?: boolean
  onClose: () => void
  onRemoved: () => void
  onRecovered?: (version: string) => void
}
export function PluginRemoveDialog(props: Props) {
  const { profile, status } = useIywAccount()
  const [transportKey, setTransportKey] = useState(0)
  useEffect(
    () => onTransportChange(() => setTransportKey((value) => value + 1)),
    []
  )
  return (
    <RemoveContent
      key={JSON.stringify([
        props.id,
        profile?.user_id,
        profile?.org_code,
        profile?.workspace_pending,
        status,
        transportKey,
      ])}
      {...props}
    />
  )
}

function RemoveContent(props: Props) {
  const t = useTranslations("PluginRemoval"),
    state = usePluginRemove(props.id, !!props.history, {
      onRemoved: props.onRemoved,
      onRecovered: props.onRecovered ?? (() => {}),
    })
  const [clear, setClear] = useState(false)
  return (
    <Dialog
      open
      onOpenChange={(open) => {
        if (!open && !state.committing) props.onClose()
      }}
    >
      <DialogContent
        showCloseButton={!state.committing}
        className="flex max-h-[85dvh] flex-col overflow-hidden"
      >
        <DialogHeader>
          <DialogTitle>{t("title")}</DialogTitle>
          <DialogDescription>{t("hint")}</DialogDescription>
        </DialogHeader>
        <RemoveBody
          state={state}
          clear={clear}
          setClear={setClear}
          onClose={props.onClose}
        />
        <RemoveActions
          state={state}
          clear={clear && !!state.view?.clearAvailable}
          onClose={props.onClose}
        />
      </DialogContent>
    </Dialog>
  )
}

function RemoveActions({
  state,
  clear,
  onClose,
}: {
  state: ReturnType<typeof usePluginRemove>
  clear: boolean
  onClose: () => void
}) {
  const t = useTranslations("PluginRemoval")
  return (
    <DialogFooter>
      <Button variant="outline" disabled={state.committing} onClick={onClose}>
        {t("close")}
      </Button>
      {state.view?.state === "ready" && !state.error ? (
        <Button
          variant="destructive"
          disabled={state.busy}
          onClick={() => void state.commit(clear)}
        >
          {t("confirm")}
        </Button>
      ) : null}
      {state.view?.state === "cleanup_pending" ? (
        <Button disabled={state.busy} onClick={() => void state.commit(false)}>
          {t("continueCleanup")}
        </Button>
      ) : null}
      {state.view?.state === "recovery_required" ? (
        <Button disabled={state.busy} onClick={() => void state.commit(false)}>
          {t("recover")}
        </Button>
      ) : null}
      {state.error || state.view?.state === "ready" ? (
        <Button
          variant="outline"
          disabled={state.busy}
          onClick={() => void state.inspect()}
        >
          {t("inspectAgain")}
        </Button>
      ) : null}
    </DialogFooter>
  )
}

function RemoveBody({
  state,
  clear,
  setClear,
  onClose,
}: {
  state: ReturnType<typeof usePluginRemove>
  clear: boolean
  setClear: (value: boolean) => void
  onClose: () => void
}) {
  const t = useTranslations("PluginRemoval")
  return (
    <div className="min-h-0 space-y-4 overflow-y-auto">
      {state.busy ? (
        <p role="status" className="text-sm">
          {t("working")}
        </p>
      ) : null}
      {state.view ? (
        <PluginRemoveDetails
          view={state.view}
          disabled={state.busy}
          onNavigate={onClose}
        />
      ) : null}
      <DataChoice state={state} clear={clear} setClear={setClear} />
      {state.error ? (
        <p role="alert" className="break-words text-sm text-destructive">
          {state.error}
        </p>
      ) : null}
      {!state.view && !state.busy && !state.error ? (
        <p className="text-sm">{t("noRecord")}</p>
      ) : null}
    </div>
  )
}

function DataChoice({
  state,
  clear,
  setClear,
}: {
  state: ReturnType<typeof usePluginRemove>
  clear: boolean
  setClear: (value: boolean) => void
}) {
  const t = useTranslations("PluginRemoval")
  return (
    <>
      {state.view?.state === "ready" ? (
        <label className="flex items-start gap-3 text-sm">
          <input
            type="checkbox"
            checked={clear && state.view.clearAvailable}
            disabled={state.busy || !state.view.clearAvailable}
            className="mt-1 size-4 shrink-0"
            onChange={(event) => setClear(event.target.checked)}
          />
          <span>
            {t("clear", { count: state.view.parameterRecords })}
            <span className="mt-1 block text-muted-foreground">
              {t(state.view.clearAvailable ? "clearHint" : "clearUnavailable")}
            </span>
          </span>
        </label>
      ) : null}
    </>
  )
}
