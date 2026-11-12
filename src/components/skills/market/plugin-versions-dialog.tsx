"use client"

import { useTranslations } from "next-intl"
import { Button } from "@/components/ui/button"
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogDescription,
  DialogFooter,
} from "@/components/ui/dialog"
import { usePluginVersions } from "./use-plugin-versions"
import { PluginVersionListRows } from "./plugin-version-list"
import { PluginVersionReviewDetails } from "./plugin-version-review"

interface Props {
  id: string
  onClose: () => void
  onChanged: () => void
  onReinstall: () => void
}

export function PluginVersionsDialog(props: Props) {
  const t = useTranslations("PluginVersions"),
    state = usePluginVersions(props.id)
  const close = () => {
    props.onClose()
    if (state.view?.state === "applied") props.onChanged()
  }
  return (
    <Dialog
      open
      onOpenChange={(open) => {
        if (!open && !state.applying) close()
      }}
    >
      <DialogContent
        showCloseButton={!state.applying}
        className="flex max-h-[85dvh] flex-col overflow-hidden"
      >
        <DialogHeader>
          <DialogTitle>{t("title")}</DialogTitle>
          <DialogDescription>{t("hint")}</DialogDescription>
        </DialogHeader>
        <VersionContent state={state} onClose={close} />
        <VersionActions
          state={state}
          onClose={close}
          onReinstall={() => {
            close()
            props.onReinstall()
          }}
        />
      </DialogContent>
    </Dialog>
  )
}

function VersionActions({
  state,
  onClose,
  onReinstall,
}: {
  state: ReturnType<typeof usePluginVersions>
  onClose: () => void
  onReinstall: () => void
}) {
  const t = useTranslations("PluginVersions")
  return (
    <DialogFooter>
      <Button variant="outline" disabled={state.applying} onClick={onClose}>
        {t("close")}
      </Button>
      {state.view?.state === "ready" && !state.error ? (
        <Button disabled={state.busy} onClick={() => void state.apply()}>
          {t("confirm")}
        </Button>
      ) : null}
      {state.view?.state !== "applied" ? (
        <Button
          variant="outline"
          disabled={state.busy || state.loading}
          onClick={() => void state.load(state.page)}
        >
          {t(state.view ? "back" : "reload")}
        </Button>
      ) : null}
      {state.error && state.view ? (
        <Button
          variant="outline"
          disabled={state.busy}
          onClick={() => void state.inspect(state.view!.version)}
        >
          {t("inspectAgain")}
        </Button>
      ) : null}
      {state.error || state.view?.state === "blocked" ? (
        <Button variant="outline" disabled={state.busy} onClick={onReinstall}>
          {t("reinstall")}
        </Button>
      ) : null}
    </DialogFooter>
  )
}

function VersionContent({
  state,
  onClose,
}: {
  state: ReturnType<typeof usePluginVersions>
  onClose: () => void
}) {
  const t = useTranslations("PluginVersions")
  return (
    <div className="min-h-0 space-y-4 overflow-y-auto">
      {state.busy ? (
        <p role="status" className="text-sm">
          {t("working")}
        </p>
      ) : null}
      {state.view ? (
        <PluginVersionReviewDetails
          view={state.view}
          busy={state.busy}
          onNavigate={onClose}
        />
      ) : (
        <PluginVersionListRows state={state} />
      )}
      {state.error ? (
        <p role="alert" className="break-words text-sm text-destructive">
          {state.error}
        </p>
      ) : null}
    </div>
  )
}
