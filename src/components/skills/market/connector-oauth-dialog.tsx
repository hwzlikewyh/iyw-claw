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
import { useConnectorOAuth } from "./use-connector-oauth"

export function ConnectorOAuthDialog({
  id,
  onClose,
}: {
  id: string
  onClose: () => void
}) {
  const t = useTranslations("ConnectorMarket")
  const state = useConnectorOAuth(id)
  return (
    <Dialog
      open
      onOpenChange={(open) => {
        if (!open) onClose()
      }}
    >
      <DialogContent>
        <DialogHeader>
          <DialogTitle>{t("authorize")}</DialogTitle>
          <DialogDescription>{t("oauthHint")}</DialogDescription>
        </DialogHeader>
        <p className="text-sm" role="status">
          {state.status
            ? t(`oauth_${state.status.state}`)
            : t("oauth_starting")}
        </p>
        {state.error || state.status?.error ? (
          <p role="alert" className="break-words text-sm text-destructive">
            {state.error || state.status?.error}
          </p>
        ) : null}
        <OAuthActions state={state} />
        <Button variant="outline" onClick={onClose}>
          {t("close")}
        </Button>
      </DialogContent>
    </Dialog>
  )
}

function OAuthActions({
  state,
}: {
  state: ReturnType<typeof useConnectorOAuth>
}) {
  const t = useTranslations("ConnectorMarket")
  const failed =
    state.error ||
    (state.status &&
      ["failed", "cancelled", "expired"].includes(state.status.state))
  return (
    <>
      {state.status?.state === "pending" && state.status.authorization_url ? (
        <Button onClick={state.open}>{t("openAuthorization")}</Button>
      ) : null}
      {failed ? (
        <Button disabled={state.restarting} onClick={() => void state.retry()}>
          {t("retryAuthorization")}
        </Button>
      ) : null}
    </>
  )
}
