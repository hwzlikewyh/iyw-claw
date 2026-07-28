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
import type { AgentSkillItem } from "@/lib/types"

type Props = {
  skill: AgentSkillItem | null
  open: boolean
  busy: boolean
  onOpenChange: (open: boolean) => void
  onConfirm: () => Promise<void>
}

export function SkillMarketUninstallDialog(props: Props) {
  const t = useTranslations("SkillsSettings.market")
  return (
    <AlertDialog open={props.open} onOpenChange={props.onOpenChange}>
      <AlertDialogContent>
        <AlertDialogHeader>
          <AlertDialogTitle>{t("installed.uninstallTitle")}</AlertDialogTitle>
          <AlertDialogDescription>
            {t("installed.uninstallDescription", {
              name: props.skill?.name ?? "",
            })}
          </AlertDialogDescription>
        </AlertDialogHeader>
        <AlertDialogFooter>
          <AlertDialogCancel disabled={props.busy}>
            {t("actions.cancel")}
          </AlertDialogCancel>
          <AlertDialogAction
            variant="destructive"
            disabled={props.busy || !props.skill}
            onClick={(event) => {
              event.preventDefault()
              void props.onConfirm().catch(() => {})
            }}
          >
            {props.busy ? t("actions.deleting") : t("actions.uninstall")}
          </AlertDialogAction>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  )
}
