"use client"

import { useTranslations } from "next-intl"
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogTitle,
} from "@/components/ui/dialog"
import {
  SkillMarketDetail,
  type SkillMarketDetailProps,
} from "@/components/skills/market/detail"

interface SkillMarketDetailDialogProps extends SkillMarketDetailProps {
  open: boolean
  onOpenChange: (open: boolean) => void
}

export function SkillMarketDetailDialog({
  open,
  onOpenChange,
  ...detailProps
}: SkillMarketDetailDialogProps) {
  const t = useTranslations("SkillMarketV2")
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="flex h-[min(46rem,calc(100dvh-2rem))] w-[min(60rem,calc(100vw-2rem))] max-w-none flex-col gap-0 overflow-hidden rounded-lg p-0">
        <DialogTitle className="sr-only">
          {detailProps.detail?.displayName ?? t("catalogTitle")}
        </DialogTitle>
        <DialogDescription className="sr-only">
          {detailProps.detail?.summary ?? t("detail.selectHint")}
        </DialogDescription>
        <SkillMarketDetail {...detailProps} />
      </DialogContent>
    </Dialog>
  )
}
