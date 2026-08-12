"use client"

import { useRef, useState } from "react"
import { useTranslations } from "next-intl"
import { KeyRound, Loader2, Trash2 } from "lucide-react"

import { Button } from "@/components/ui/button"
import type { PendingChannelConfirmationState } from "@/lib/types"

interface ChannelConfirmationCardProps {
  confirmation: PendingChannelConfirmationState
  onRespond: (
    confirmationId: string,
    confirmed: boolean
  ) => void | Promise<void>
}

export function ChannelConfirmationCard({
  confirmation,
  onRespond,
}: ChannelConfirmationCardProps) {
  const t = useTranslations("Folder.chat.channelConfirmation")
  const [submitting, setSubmitting] = useState(false)
  const [failed, setFailed] = useState(false)
  const inFlight = useRef(false)
  const deletesChannel = confirmation.action === "delete_channel"
  const Icon = deletesChannel ? Trash2 : KeyRound

  const respond = async (confirmed: boolean) => {
    if (inFlight.current) return
    inFlight.current = true
    setSubmitting(true)
    setFailed(false)
    try {
      await onRespond(confirmation.confirmation_id, confirmed)
      inFlight.current = false
    } catch {
      inFlight.current = false
      setSubmitting(false)
      setFailed(true)
    }
  }

  return (
    <div
      role="alertdialog"
      aria-labelledby={`${confirmation.confirmation_id}-title`}
      className="mb-2 rounded-lg border border-destructive/30 bg-card p-3 shadow-lg"
    >
      <div className="flex items-start gap-2.5">
        <span className="flex size-8 shrink-0 items-center justify-center rounded-lg bg-destructive/10 text-destructive">
          <Icon className="size-4" />
        </span>
        <div className="min-w-0 flex-1">
          <p
            id={`${confirmation.confirmation_id}-title`}
            className="text-sm font-medium"
          >
            {deletesChannel
              ? t("deleteChannelTitle")
              : t("deleteCredentialTitle")}
          </p>
          <p className="mt-0.5 text-xs text-muted-foreground">
            {deletesChannel
              ? t("deleteChannelDescription", {
                  count: confirmation.local_record_count,
                })
              : t("deleteCredentialDescription")}
          </p>
          <dl className="mt-3 grid grid-cols-[auto_minmax(0,1fr)] gap-x-3 gap-y-1 text-xs">
            <dt className="text-muted-foreground">{t("channel")}</dt>
            <dd className="truncate font-medium">
              {confirmation.channel_name}
            </dd>
            <dt className="text-muted-foreground">{t("type")}</dt>
            <dd className="truncate">{confirmation.channel_type}</dd>
            <dt className="text-muted-foreground">{t("status")}</dt>
            <dd>{confirmation.enabled ? t("enabled") : t("disabled")}</dd>
          </dl>
        </div>
      </div>
      <div className="mt-3 flex items-center justify-end gap-2">
        {failed && (
          <span role="alert" className="mr-auto text-xs text-destructive">
            {t("submitError")}
          </span>
        )}
        <Button
          variant="outline"
          size="sm"
          disabled={submitting}
          onClick={() => void respond(false)}
        >
          {t("cancel")}
        </Button>
        <Button
          variant="destructive"
          size="sm"
          disabled={submitting}
          onClick={() => void respond(true)}
        >
          {submitting && <Loader2 className="animate-spin" />}
          {t("confirmDelete")}
        </Button>
      </div>
    </div>
  )
}
