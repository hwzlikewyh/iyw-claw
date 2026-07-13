"use client"

import { useTranslations } from "next-intl"

import {
  AlertDialog,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from "@/components/ui/alert-dialog"
import { Button } from "@/components/ui/button"

interface AgentStorageConfirmationDialogProps {
  kind: "system" | "global" | null
  onCancel: () => void
  onConfirm: () => void
}

export function AgentStorageConfirmationDialog({
  kind,
  onCancel,
  onConfirm,
}: AgentStorageConfirmationDialogProps) {
  const t = useTranslations("AcpAgentSettings")
  return (
    <AlertDialog open={kind !== null} onOpenChange={onCancel}>
      <AlertDialogContent>
        <AlertDialogHeader>
          <AlertDialogTitle>{t("storage.confirmTitle")}</AlertDialogTitle>
          <AlertDialogDescription>
            {kind === "global"
              ? t("storage.globalProfileWarning")
              : t("storage.systemDriveWarning")}
          </AlertDialogDescription>
        </AlertDialogHeader>
        <AlertDialogFooter>
          <AlertDialogCancel>{t("storage.cancel")}</AlertDialogCancel>
          <Button onClick={onConfirm}>{t("storage.confirm")}</Button>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  )
}
