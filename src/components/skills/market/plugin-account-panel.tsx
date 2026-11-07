"use client"

import { useState } from "react"
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
import type {
  PluginConnectorPreparation,
  PluginPreparationChange,
} from "@/lib/plugin-preparation"
import { usePluginAccount } from "./use-plugin-account"

interface Props {
  input: PluginPreparationChange
  connector: PluginConnectorPreparation
  disabled: boolean
  onChanged: () => void
}
type AccountState = ReturnType<typeof usePluginAccount>

export function PluginAccountPanel(props: Props) {
  const t = useTranslations("PluginPreparation")
  const state = usePluginAccount(
    { ...props.input, connectorKey: props.connector.key },
    props.onChanged
  )
  const [logout, setLogout] = useState(false)
  return (
    <div className="space-y-2 text-[13px]">
      <p role="status">{t(accountCaption(state))}</p>
      {state.error ? (
        <p role="alert" className="break-words text-destructive">
          {state.error}
        </p>
      ) : null}
      <AccountActions
        state={state}
        connector={props.connector}
        disabled={props.disabled}
        onLogout={() => setLogout(true)}
      />
      {logout ? (
        <AccountLogout
          busy={state.busy}
          error={state.error}
          onClose={() => setLogout(false)}
          onConfirm={() =>
            void state.act("logout").then((success) => {
              if (success) setLogout(false)
            })
          }
        />
      ) : null}
    </div>
  )
}

function accountCaption(state: AccountState) {
  if (state.busy) return "working"
  if (state.expired) return "accountExpired"
  if (state.pending) return "accountWaiting"
  return state.status
    ? (`accountState.${state.status.state}` as const)
    : "accountUnchecked"
}

function AccountActions({
  state,
  connector,
  disabled,
  onLogout,
}: {
  state: AccountState
  connector: PluginConnectorPreparation
  disabled: boolean
  onLogout: () => void
}) {
  const t = useTranslations("PluginPreparation"),
    blocked = disabled || state.busy || state.pending
  return (
    <div className="flex flex-wrap gap-2">
      <Button
        size="sm"
        variant="outline"
        disabled={blocked}
        onClick={() => void state.act("status")}
      >
        {t("checkAccount")}
      </Button>
      {connector.canLogin ? (
        <Button
          size="sm"
          disabled={blocked}
          onClick={() => void state.act("login")}
        >
          {t("loginAccount")}
        </Button>
      ) : null}
      <AccountWaiting state={state} />
      {connector.canLogout ? (
        <Button
          size="sm"
          variant="outline"
          disabled={blocked}
          onClick={onLogout}
        >
          {t("logoutAccount")}
        </Button>
      ) : null}
    </div>
  )
}

function AccountWaiting({ state }: { state: AccountState }) {
  const t = useTranslations("PluginPreparation")
  return (
    <>
      {state.pending ? (
        <>
          <Button size="sm" variant="outline" onClick={state.stop}>
            {t("stopWaiting")}
          </Button>
          {state.status?.authorizationUrl ? (
            <Button
              size="sm"
              variant="outline"
              onClick={() => void state.open()}
            >
              {t("openLogin")}
            </Button>
          ) : null}
        </>
      ) : null}
    </>
  )
}

interface LogoutProps {
  busy: boolean
  error: string
  onClose: () => void
  onConfirm: () => void
}
function AccountLogout({ busy, error, onClose, onConfirm }: LogoutProps) {
  const t = useTranslations("PluginPreparation")
  return (
    <Dialog
      open
      onOpenChange={(open) => {
        if (!open && !busy) onClose()
      }}
    >
      <DialogContent>
        <DialogHeader>
          <DialogTitle>{t("logoutTitle")}</DialogTitle>
          <DialogDescription>{t("logoutHint")}</DialogDescription>
        </DialogHeader>
        {error ? (
          <p role="alert" className="break-words text-sm text-destructive">
            {error}
          </p>
        ) : null}
        <DialogFooter>
          <Button variant="outline" disabled={busy} onClick={onClose}>
            {t("cancel")}
          </Button>
          <Button variant="destructive" disabled={busy} onClick={onConfirm}>
            {t(busy ? "saving" : "confirmLogout")}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}
