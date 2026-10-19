"use client"

import { useTranslations } from "next-intl"
import { MessageResponse } from "@/components/ai-elements/message"
import {
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"

export function CompactionSummaryDialog({
  summary,
  running,
}: {
  summary: string
  running: boolean
}) {
  const t = useTranslations("Folder.chat.contentParts.contextCompaction")
  return (
    <DialogContent className="max-w-3xl grid-rows-[auto_minmax(0,1fr)] gap-4 overflow-hidden rounded-2xl">
      <DialogHeader>
        <DialogTitle>{t("viewSummary")}</DialogTitle>
        <DialogDescription>{t("summaryDescription")}</DialogDescription>
      </DialogHeader>
      <div
        tabIndex={0}
        className="min-h-0 max-h-[65dvh] overflow-y-auto rounded-lg border bg-muted/20 p-4 text-sm break-words focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
      >
        <MessageResponse mode={running ? "streaming" : "static"}>
          {summary}
        </MessageResponse>
      </div>
    </DialogContent>
  )
}
