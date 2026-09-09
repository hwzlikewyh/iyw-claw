"use client"

import { memo, useEffect, useState } from "react"
import { useTranslations } from "next-intl"
import { formatElapsedLabel } from "@/lib/format-elapsed"
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from "@/components/ui/tooltip"

const TICK_MS = 1_000
const SECONDS_PER_MINUTE = 60

function elapsedSeconds(startedAt: number | null) {
  return startedAt === null
    ? 0
    : Math.max(0, Math.floor((Date.now() - startedAt) / TICK_MS))
}

export const LiveTurnElapsed = memo(function LiveTurnElapsed({
  startedAt,
}: {
  startedAt: number | null
}) {
  const t = useTranslations("Folder.chat.liveTurnStats")
  const [seconds, setSeconds] = useState(() => elapsedSeconds(startedAt))
  useEffect(() => {
    if (startedAt === null) return
    let timer: ReturnType<typeof setInterval> | undefined
    const sync = () => {
      clearInterval(timer)
      if (document.hidden) return
      setSeconds(elapsedSeconds(startedAt))
      timer = setInterval(() => setSeconds(elapsedSeconds(startedAt)), TICK_MS)
    }
    sync()
    document.addEventListener("visibilitychange", sync)
    return () => {
      clearInterval(timer)
      document.removeEventListener("visibilitychange", sync)
    }
  }, [startedAt])
  if (startedAt === null) return null
  const minutes = String(Math.floor(seconds / SECONDS_PER_MINUTE)).padStart(
    2,
    "0"
  )
  const remainder = String(seconds % SECONDS_PER_MINUTE).padStart(2, "0")
  const elapsed = formatElapsedLabel(seconds * TICK_MS, t)
  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <span
          tabIndex={0}
          aria-label={t("elapsed", { elapsed })}
          className="inline-flex h-8 min-w-10 items-center justify-end whitespace-nowrap text-xs text-muted-foreground tabular-nums"
        >
          {minutes}:{remainder}
        </span>
      </TooltipTrigger>
      <TooltipContent>{t("elapsed", { elapsed })}</TooltipContent>
    </Tooltip>
  )
})
