"use client"

import { Pencil, RotateCw, SquareArrowOutUpRight, X } from "lucide-react"
import { useTranslations } from "next-intl"
import { Button } from "@/components/ui/button"
import type { AutomationRun } from "@/lib/types"
import { unavailableAutomationModel } from "./automation-run-error"

export function AutomationRunActions({
  run,
  busy,
  active,
  onRetry,
  onCancel,
  onEdit,
  onOpen,
  onRelated,
}: {
  run: AutomationRun
  busy: boolean
  active: boolean
  onRetry: () => void
  onCancel: () => void
  onEdit?: () => void
  onOpen?: () => void
  onRelated: (id: number) => void
}) {
  const t = useTranslations("Automations")
  return (
    <div className="flex flex-wrap items-center justify-end gap-2 text-xs">
      {run.status === "failed" &&
        unavailableAutomationModel(run.error) &&
        onEdit && (
          <Button
            size="sm"
            variant="ghost"
            className="h-7 text-xs text-primary"
            disabled={busy || active}
            onClick={onEdit}
          >
            <Pencil className="size-3" />
            {t("changeModel")}
          </Button>
        )}
      {run.retry_of_run_id != null && (
        <Button
          size="sm"
          variant="link"
          className="h-7 px-0 text-[10px] text-muted-foreground"
          onClick={() => onRelated(run.retry_of_run_id!)}
        >
          {t("retrySource", { id: run.retry_of_run_id })}
        </Button>
      )}
      {run.retry_run_id != null ? (
        <Button
          size="sm"
          variant="link"
          className="h-7 px-0 text-[10px]"
          onClick={() => onRelated(run.retry_run_id!)}
        >
          {t("retryLinked", { id: run.retry_run_id })}
        </Button>
      ) : (
        run.status === "failed" && (
          <Button
            size="sm"
            variant="ghost"
            className="h-7 text-xs text-primary"
            disabled={busy || active}
            onClick={onRetry}
          >
            <RotateCw className="size-3" />
            {t("retryRun")}
          </Button>
        )
      )}
      {run.status === "running" && (
        <Button
          size="sm"
          variant="ghost"
          className="h-7 text-xs text-destructive"
          disabled={busy}
          onClick={onCancel}
        >
          <X className="size-3" />
          {t("cancelRun")}
        </Button>
      )}
      {onOpen ? (
        <Button
          size="sm"
          variant="link"
          className="h-7 px-0 text-xs"
          onClick={onOpen}
        >
          <SquareArrowOutUpRight className="size-3" />
          {t("viewConversation")}
        </Button>
      ) : (
        <span className="text-[10px] text-muted-foreground">
          {t("noConversation")}
        </span>
      )}
    </div>
  )
}

export function AutomationRunNotification({ run }: { run: AutomationRun }) {
  const t = useTranslations("Automations")
  if (!run.notification_channel_id) return null
  const keys = {
    sent: "notificationSent",
    failed: "notificationFailed",
    pending: "notificationPending",
    sending: "notificationSending",
    queued: "notificationQueued",
  } as const
  const key = keys[run.notification_status as keyof typeof keys]
  return (
    <span
      title={
        run.notification_error || run.notification_channel_name || undefined
      }
      className={
        run.notification_status === "failed"
          ? "text-[10px] text-amber-600 dark:text-amber-400"
          : "text-[10px] text-muted-foreground"
      }
    >
      {key ? t(key) : t("notificationState")}
      {run.notification_channel_name
        ? ` · ${run.notification_channel_name}`
        : ""}
    </span>
  )
}
