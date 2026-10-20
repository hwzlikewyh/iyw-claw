"use client"

import { useLocale, useTranslations } from "next-intl"
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"
import type { AutomationRun } from "@/lib/types"
import { AutomationStatus, dateTime } from "./automation-presentation"
import { AutomationRunNotification } from "./automation-run-actions"
import { useAutomationErrorMessage } from "./automation-run-error"

export function AutomationRelatedRun({
  run,
  onClose,
}: {
  run: AutomationRun
  onClose: () => void
}) {
  const t = useTranslations("Automations")
  const locale = useLocale()
  const errorMessage = useAutomationErrorMessage()
  return (
    <Dialog
      open
      onOpenChange={(open) => {
        if (!open) onClose()
      }}
    >
      <DialogContent>
        <DialogHeader>
          <DialogTitle>{t("runNumber", { id: run.id })}</DialogTitle>
          <DialogDescription>
            {dateTime(run.started_at || run.created_at, locale)}
          </DialogDescription>
        </DialogHeader>
        <AutomationStatus status={run.status} retry={run.trigger === "retry"} />
        <p className="whitespace-pre-wrap text-xs leading-relaxed text-muted-foreground">
          {run.error
            ? errorMessage(run.error)
            : run.summary || t("noConversation")}
        </p>
        <AutomationRunNotification run={run} />
      </DialogContent>
    </Dialog>
  )
}
