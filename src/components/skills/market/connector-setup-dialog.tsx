"use client"

import { useTranslations } from "next-intl"
import { Button } from "@/components/ui/button"
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"
import { ConnectorOAuthContent } from "./connector-oauth-dialog"
import { ConnectorCLIContent } from "./connector-cli-dialog"
import { useConnectorSetup } from "./use-connector-setup"

type Setup = ReturnType<typeof useConnectorSetup>
type Props = {
  id: string
  name: string
  mode: "oauth" | "cli"
  autoAuthorize: boolean
  onClose: () => void
  onConfigure: () => void
  onManageInstalled: () => void
}

export function ConnectorSetupDialog(props: Props) {
  const t = useTranslations("ConnectorMarket")
  const state = useConnectorSetup(props.id, props.autoAuthorize)
  return (
    <Dialog
      open
      onOpenChange={(open) => {
        if (!open) props.onClose()
      }}
    >
      <DialogContent className="flex max-h-[90dvh] flex-col overflow-y-auto sm:max-w-3xl">
        <DialogHeader>
          <DialogTitle>{props.name}</DialogTitle>
          <DialogDescription>{t("setupHint")}</DialogDescription>
        </DialogHeader>
        {state.phase === "authorizing" ? (
          props.mode === "oauth" ? (
            <ConnectorOAuthContent id={props.id} onComplete={state.verify} />
          ) : (
            <ConnectorCLIContent id={props.id} onChecked={state.checked} />
          )
        ) : (
          <SetupResult state={state} />
        )}
        <SetupFooter {...props} state={state} />
      </DialogContent>
    </Dialog>
  )
}

function SetupResult({ state }: { state: Setup }) {
  const t = useTranslations("ConnectorMarket")
  return (
    <div className="space-y-2 text-sm">
      <p role="status">
        {state.phase === "checking"
          ? t("checkingConnection")
          : state.view
            ? t(`connection_${state.view.state}`)
            : t("setupIncomplete")}
      </p>
      {state.view && (
        <p className="text-muted-foreground">
          {t(`connectionHint_${state.view.state}`)}
        </p>
      )}
      {state.error && (
        <p role="alert" className="break-words text-destructive">
          {state.error}
        </p>
      )}
    </div>
  )
}

function SetupFooter(props: Props & { state: Setup }) {
  const t = useTranslations("ConnectorMarket")
  const ready = props.state.view?.state === "ready"
  return (
    <DialogFooter className="flex-wrap gap-2">
      <Button variant="outline" onClick={props.onClose}>
        {ready ? t("close") : t("finishLater")}
      </Button>
      {props.state.phase === "result" && (
        <>
          <Button variant="outline" onClick={props.onConfigure}>
            {t("configure")}
          </Button>
          {ready ? (
            <Button onClick={props.onManageInstalled}>
              {t("manageInstalled")}
            </Button>
          ) : (
            <>
              <Button variant="outline" onClick={props.state.verify}>
                {t("checkConnection")}
              </Button>
              <Button onClick={props.state.authorize}>
                {t("continueSetup")}
              </Button>
            </>
          )}
        </>
      )}
    </DialogFooter>
  )
}
