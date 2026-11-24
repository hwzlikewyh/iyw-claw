"use client"

import { ChevronLeft, ChevronRight } from "lucide-react"
import { useTranslations } from "next-intl"
import { Button } from "@/components/ui/button"
import type { ImagePreviewNavigation } from "./image-editor-model"

export function ImageSequenceControls({
  navigation,
  counter = true,
}: {
  navigation?: ImagePreviewNavigation
  counter?: boolean
}) {
  const t = useTranslations("Folder.chat.messageList")
  if (!navigation || navigation.total <= 1) return null
  return (
    <div className="flex shrink-0 items-center gap-1">
      <Button
        type="button"
        variant="ghost"
        size="icon-sm"
        aria-label={t("imageViewerPrevious")}
        title={t("imageViewerPrevious")}
        disabled={navigation.index <= 0}
        onClick={() => navigation.onIndexChange(navigation.index - 1)}
      >
        <ChevronLeft />
      </Button>
      {counter && (
        <span
          className="text-xs tabular-nums text-muted-foreground"
          aria-live="polite"
        >
          {navigation.index + 1} / {navigation.total}
        </span>
      )}
      <Button
        type="button"
        variant="ghost"
        size="icon-sm"
        aria-label={t("imageViewerNext")}
        title={t("imageViewerNext")}
        disabled={navigation.index >= navigation.total - 1}
        onClick={() => navigation.onIndexChange(navigation.index + 1)}
      >
        <ChevronRight />
      </Button>
    </div>
  )
}
