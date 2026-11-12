"use client"

import { useTranslations } from "next-intl"
import { Button } from "@/components/ui/button"
import { DialogFooter } from "@/components/ui/dialog"
import type { usePluginRecovery } from "./use-plugin-recovery"

interface Props {
  state: ReturnType<typeof usePluginRecovery>
  inspected: boolean
  onInspect: () => void
  onClose: () => void
  onReinstall: () => void
}

export function RecoveryActions(props: Props) {
  const t = useTranslations("PluginRecovery")
  const { state, inspected } = props
  return (
    <DialogFooter className="shrink-0">
      <Button variant="outline" disabled={state.busy} onClick={props.onClose}>
        {t("close")}
      </Button>
      {state.view?.state === "ready" && !state.error ? (
        <Button disabled={state.busy} onClick={() => void state.apply()}>
          {t("confirm")}
        </Button>
      ) : null}
      {state.view?.state !== "recovered" ? (
        <Button
          variant="outline"
          disabled={state.busy}
          onClick={() => {
            props.onInspect()
          }}
        >
          {t(inspected ? "inspectAgain" : "inspect")}
        </Button>
      ) : null}
      {state.error || state.view?.state === "blocked" ? (
        <Button
          variant="outline"
          disabled={state.busy}
          onClick={() => {
            props.onClose()
            props.onReinstall()
          }}
        >
          {t("reinstall")}
        </Button>
      ) : null}
    </DialogFooter>
  )
}
