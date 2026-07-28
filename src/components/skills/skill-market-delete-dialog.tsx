"use client"

import { useTranslations } from "next-intl"
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from "@/components/ui/alert-dialog"
import type { SkillMarketDetail } from "@/lib/skill-market"

export function SkillMarketDeleteDialog({
  detail,
  open,
  busy,
  onOpenChange,
  onDelete,
}: {
  detail: SkillMarketDetail | null
  open: boolean
  busy: boolean
  onOpenChange: (open: boolean) => void
  onDelete: () => Promise<void>
}) {
  const t = useTranslations("SkillsSettings.market")
  return (
    <AlertDialog open={open} onOpenChange={onOpenChange}>
      <AlertDialogContent>
        <AlertDialogHeader>
          <AlertDialogTitle>{t("delete.title")}</AlertDialogTitle>
          <AlertDialogDescription>
            {t("delete.description", { name: detail?.displayName ?? "" })}
          </AlertDialogDescription>
        </AlertDialogHeader>
        <AlertDialogFooter>
          <AlertDialogCancel disabled={busy}>
            {t("actions.cancel")}
          </AlertDialogCancel>
          <AlertDialogAction
            variant="destructive"
            disabled={busy || !detail}
            onClick={(event) => {
              event.preventDefault()
              void onDelete().catch(() => {})
            }}
          >
            {busy ? t("actions.deleting") : t("actions.delete")}
          </AlertDialogAction>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  )
}
