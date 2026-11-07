"use client"

import { useTranslations } from "next-intl"
import { Button } from "@/components/ui/button"
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"
import { TerminalView } from "@/components/terminal/terminal-view"
import type { ConnectorCheck } from "@/lib/connector-auth"
import { useConnectorCLI } from "./use-connector-cli"

type CLIState = ReturnType<typeof useConnectorCLI>

export function ConnectorCLIDialog({
  id,
  onClose,
  onChecked,
}: {
  id: string
  onClose: () => void
  onChecked: (value: ConnectorCheck) => void
}) {
  const t = useTranslations("ConnectorMarket")
  const state = useConnectorCLI(id, onChecked)
  return (
    <Dialog
      open
      onOpenChange={(open) => {
        if (!open) onClose()
      }}
    >
      <DialogContent className="flex max-h-[90dvh] flex-col overflow-hidden sm:max-w-3xl">
        <DialogHeader>
          <DialogTitle>{t("cliLogin")}</DialogTitle>
          <DialogDescription>{t("cliHint")}</DialogDescription>
        </DialogHeader>
        {state.terminal ? (
          <div className="relative h-[min(45dvh,22rem)] min-h-40 shrink-0 overflow-hidden rounded-md bg-background">
            <TerminalView
              terminalId={state.terminal}
              workingDir=""
              isActive
              isVisible
              spawnProcess={state.spawn}
              onProcessExited={() => void state.verify()}
            />
          </div>
        ) : null}
        <CLIStatus state={state} />
        <CLIControls state={state} onClose={onClose} />
      </DialogContent>
    </Dialog>
  )
}

function CLIStatus({ state }: { state: CLIState }) {
  const t = useTranslations("ConnectorMarket")
  return (
    <>
      <p role="status" className="break-words text-sm">
        {state.checking
          ? t("checkingConnection")
          : state.view
            ? t(`connection_${state.view.state}`)
            : t("cliNotVerified")}
      </p>
      {state.error ? (
        <p role="alert" className="break-words text-sm text-destructive">
          {state.error}
        </p>
      ) : null}
    </>
  )
}

function CLIControls({
  state,
  onClose,
}: {
  state: CLIState
  onClose: () => void
}) {
  const t = useTranslations("ConnectorMarket")
  return (
    <div className="flex flex-wrap gap-2">
      {!state.terminal ? (
        <Button onClick={state.start}>{t("cliStart")}</Button>
      ) : (
        <>
          <Button disabled={state.checking} onClick={() => void state.verify()}>
            {t("cliVerify")}
          </Button>
          <Button
            variant="outline"
            disabled={state.checking}
            onClick={() => void state.restart()}
          >
            {t("cliRestart")}
          </Button>
        </>
      )}
      <Button variant="outline" onClick={onClose}>
        {t("close")}
      </Button>
    </div>
  )
}
