"use client"

import { useTranslations } from "next-intl"
import { Button } from "@/components/ui/button"
import { Badge } from "@/components/ui/badge"
import type { LocalMcpServer } from "@/lib/types"
import { ConnectorCLIDialog } from "./connector-cli-dialog"
import { ConnectorOAuthDialog } from "./connector-oauth-dialog"
import { useConnectorStatus } from "./use-connector-status"

type StatusState = ReturnType<typeof useConnectorStatus>
type AuthMetadata = {
  auth_mode?: string
  cli_auth?: { plan?: { logout?: unknown } }
}

export function ConnectorInstalledStatus({
  item,
  disabled,
}: {
  item: LocalMcpServer
  disabled: boolean
}) {
  const t = useTranslations("ConnectorMarket")
  const state = useConnectorStatus(item.id)
  const meta = item.spec.iyw_market as AuthMetadata | undefined
  return (
    <>
      <Badge variant="outline" role="status">
        {state.busy
          ? t("checkingConnection")
          : state.view
            ? t(`connection_${state.view.state}`)
            : t("configured")}
      </Badge>
      <Button
        size="sm"
        variant="outline"
        disabled={state.busy || disabled}
        onClick={() => void state.inspect()}
      >
        {t("checkConnection")}
      </Button>
      <AuthenticationActions meta={meta} state={state} disabled={disabled} />
      <CheckDetails state={state} />
      <AuthenticationDialog id={item.id} mode={meta?.auth_mode} state={state} />
    </>
  )
}

function AuthenticationActions({
  meta,
  state,
  disabled,
}: {
  meta: AuthMetadata | undefined
  state: StatusState
  disabled: boolean
}) {
  const t = useTranslations("ConnectorMarket")
  return (
    <>
      {["cli", "oauth"].includes(meta?.auth_mode || "") ? (
        <Button
          size="sm"
          variant="outline"
          disabled={state.busy || disabled}
          onClick={state.open}
        >
          {meta?.auth_mode === "cli" ? t("cliLogin") : t("authorize")}
        </Button>
      ) : null}
      {meta?.auth_mode === "cli" && meta.cli_auth?.plan?.logout ? (
        <Button
          size="sm"
          variant="outline"
          disabled={state.busy || disabled}
          onClick={() => void state.inspect(true)}
        >
          {t("disconnectAccount")}
        </Button>
      ) : null}
    </>
  )
}

function CheckDetails({ state }: { state: StatusState }) {
  const t = useTranslations("ConnectorMarket")
  return (
    <>
      {state.view ? (
        <p className="basis-full text-xs text-muted-foreground">
          {t(`connectionHint_${state.view.state}`)}
          {state.view.toolCount !== null
            ? ` · ${t("toolCount", { count: state.view.toolCount })}`
            : ""}
        </p>
      ) : null}
      {state.error ? (
        <p
          role="alert"
          className="basis-full break-words text-sm text-destructive"
        >
          {state.error}
        </p>
      ) : null}
    </>
  )
}

function AuthenticationDialog({
  id,
  mode,
  state,
}: {
  id: string
  mode: string | undefined
  state: StatusState
}) {
  if (!state.dialog) return null
  if (mode === "cli")
    return (
      <ConnectorCLIDialog
        id={id}
        onClose={() => state.setDialog(false)}
        onChecked={state.setView}
      />
    )
  if (mode === "oauth")
    return (
      <ConnectorOAuthDialog
        id={id}
        onClose={() => {
          state.setDialog(false)
          void state.inspect()
        }}
      />
    )
  return null
}
