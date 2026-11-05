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

export function ConnectorMigrationDialog({
  open,
  busy,
  onCancel,
  onConfirm,
}: {
  open: boolean
  busy: boolean
  onCancel: () => void
  onConfirm: () => void
}) {
  const t = useTranslations("ConnectorMarket")
  return (
    <Dialog
      open={open}
      onOpenChange={(next) => {
        if (!next && !busy) onCancel()
      }}
    >
      <DialogContent>
        <DialogHeader>
          <DialogTitle>{t("migrationTitle")}</DialogTitle>
          <DialogDescription>{t("migrationDescription")}</DialogDescription>
        </DialogHeader>
        <DialogFooter>
          <Button variant="outline" disabled={busy} onClick={onCancel}>
            {t("cancel")}
          </Button>
          <Button disabled={busy} onClick={onConfirm}>
            {t("useInConversation")}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}
