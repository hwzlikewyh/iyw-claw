"use client"

import { useTranslations } from "next-intl"
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogTitle,
} from "@/components/ui/dialog"
import {
  InventoryDetail,
  type InventoryActions,
} from "@/components/skills/market/installed-inventory-detail"
import type { LogicalSkillInventoryItem } from "@/lib/types"

export function InstalledInventoryDialog({
  open,
  onOpenChange,
  skill,
  busyKey,
  onToggle,
  onTakeOver,
  onReconcile,
}: {
  open: boolean
  onOpenChange: (open: boolean) => void
  skill: LogicalSkillInventoryItem | null
  busyKey: string | null
} & InventoryActions) {
  const t = useTranslations("SkillMarketV2.inventory")
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="flex h-[min(52rem,calc(100dvh-2rem))] w-[min(48rem,calc(100vw-2rem))] max-w-none flex-col gap-0 overflow-hidden rounded-lg p-0">
        <DialogTitle className="sr-only">
          {skill?.name ?? t("details")}
        </DialogTitle>
        <DialogDescription className="sr-only">
          {skill?.description ?? t("detailsHint")}
        </DialogDescription>
        {skill ? (
          <InventoryDetail
            skill={skill}
            busyKey={busyKey}
            onToggle={onToggle}
            onTakeOver={onTakeOver}
            onReconcile={onReconcile}
          />
        ) : null}
      </DialogContent>
    </Dialog>
  )
}
