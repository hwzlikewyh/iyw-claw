"use client"

import { useEffect, useState } from "react"
import { Loader2 } from "lucide-react"
import { useTranslations } from "next-intl"

import { useConnection } from "@/hooks/use-connection"

const SETTLE_SYNC_DISPLAY_MS = 30_000

export function BackgroundTasksChip({ contextKey }: { contextKey: string }) {
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

  return (
    <div className="border-b border-sky-500/20 bg-sky-500/10 px-3 py-1.5 text-xs text-sky-700 dark:text-sky-300">
      <div className="mx-auto flex w-full max-w-3xl items-center justify-center gap-2">
        <Loader2 className="size-3.5 shrink-0 animate-spin" />
        <span className="min-w-0 truncate">
          {backgroundOutstanding > 0
            ? t("running", { count: backgroundOutstanding })
            : t("settling")}
        </span>
      </div>
    </div>
  )
}
