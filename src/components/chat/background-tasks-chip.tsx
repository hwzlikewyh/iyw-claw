"use client"

import { useEffect, useState } from "react"
import { Loader2 } from "lucide-react"
import { useTranslations } from "next-intl"

import { useConnection } from "@/hooks/use-connection"

const SETTLE_SYNC_DISPLAY_MS = 30_000

interface BackgroundTasksChipProps {
  contextKey: string
  inline?: boolean
}

export function BackgroundTasksChip({
  contextKey,
  inline = false,
}: BackgroundTasksChipProps) {
  const t = useTranslations("Folder.chat.backgroundTasks")
  const { backgroundOutstanding, backgroundSettleSyncingSince } =
    useConnection(contextKey)
  const [expiredFor, setExpiredFor] = useState<number | null>(null)

  useEffect(() => {
    if (backgroundSettleSyncingSince == null) return
    const remaining =
      SETTLE_SYNC_DISPLAY_MS - (Date.now() - backgroundSettleSyncingSince)
    const timer = setTimeout(
      () => setExpiredFor(backgroundSettleSyncingSince),
      Math.max(0, remaining) + 50
    )
    return () => clearTimeout(timer)
  }, [backgroundSettleSyncingSince])

  const showSyncing =
    backgroundOutstanding <= 0 &&
    backgroundSettleSyncingSince != null &&
    expiredFor !== backgroundSettleSyncingSince

  if (backgroundOutstanding <= 0 && !showSyncing) return null

  const status = (
    <span className="inline-flex min-w-0 items-center gap-1 leading-none text-sky-700 dark:text-sky-300">
      <Loader2 className="size-3 shrink-0 animate-spin" />
      <span className="min-w-0 truncate">
        {backgroundOutstanding > 0
          ? t("running", { count: backgroundOutstanding })
          : t("settling")}
      </span>
    </span>
  )

  if (inline) {
    return (
      <>
        <span className="text-border leading-none">|</span>
        {status}
      </>
    )
  }

  return (
    <div className="flex min-h-8 flex-wrap items-center justify-center gap-x-3 gap-y-1 px-4 py-1 text-xs leading-none text-muted-foreground">
      {status}
    </div>
  )
}
