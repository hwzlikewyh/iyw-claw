"use client"

import { CircleDollarSign } from "lucide-react"
import { useTranslations } from "next-intl"

import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogMedia,
  AlertDialogTitle,
} from "@/components/ui/alert-dialog"
import type { IywAccountStatus } from "@/contexts/iyw-account-context"

export type ConversationPointsBlockReason = "insufficient" | "unknown"

export function getConversationPointsBlockReason(
  status: IywAccountStatus,
  balancePoints: number | null | undefined
): ConversationPointsBlockReason | null {
  if (
    status !== "authenticated" ||
    typeof balancePoints !== "number" ||
    !Number.isFinite(balancePoints)
  ) {
    return "unknown"
  }
  return balancePoints > 0 ? null : "insufficient"
}

export function ConversationPointsDialog({
  reason,
  onDismiss,
}: {
  reason: ConversationPointsBlockReason | null
  onDismiss: () => void
}) {
  const t = useTranslations("Folder.conversation.pointsGate")
  const insufficient = reason === "insufficient"

  return (
    <AlertDialog
      open={reason !== null}
      onOpenChange={(open) => {
        if (!open) onDismiss()
      }}
    >
      <AlertDialogContent>
        <AlertDialogHeader>
          <AlertDialogMedia>
            <CircleDollarSign />
          </AlertDialogMedia>
          <AlertDialogTitle>
            {t(insufficient ? "insufficientTitle" : "unknownTitle")}
          </AlertDialogTitle>
          <AlertDialogDescription>
            {t(insufficient ? "insufficientDescription" : "unknownDescription")}
          </AlertDialogDescription>
        </AlertDialogHeader>
        <AlertDialogFooter>
          <AlertDialogAction onClick={onDismiss}>
            {t("dismiss")}
          </AlertDialogAction>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  )
}
