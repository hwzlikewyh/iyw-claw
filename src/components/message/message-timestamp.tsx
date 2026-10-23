"use client"

import { useMemo } from "react"
import { useLocale } from "next-intl"
import { LoaderCircle } from "lucide-react"

import { cn } from "@/lib/utils"

interface MessageTimestampProps {
  timestamp?: string | null
  align?: "start" | "end"
  className?: string
  pending?: boolean
}

export function MessageTimestamp({
  timestamp,
  align = "start",
  className,
  pending = false,
}: MessageTimestampProps) {
  const locale = useLocale()
  const date = useMemo(() => {
    if (!timestamp) return null
    const parsed = new Date(timestamp)
    return Number.isNaN(parsed.getTime()) ? null : parsed
  }, [timestamp])

  const formatters = useMemo(
    () => ({
      short: new Intl.DateTimeFormat(locale, {
        hour: "2-digit",
        minute: "2-digit",
      }),
      full: new Intl.DateTimeFormat(locale, {
        dateStyle: "medium",
        timeStyle: "medium",
      }),
    }),
    [locale]
  )

  if (!date || !timestamp) return null

  return (
    <time
      dateTime={timestamp}
      title={formatters.full.format(date)}
      className={cn(
        "inline-flex items-center gap-1.5 text-[11px] leading-none tabular-nums text-muted-foreground/70",
        align === "end" ? "self-end text-end" : "self-start",
        className
      )}
    >
      {formatters.short.format(date)}
      {pending && (
        <LoaderCircle
          aria-hidden="true"
          className="size-3 conversation-working-mark"
        />
      )}
    </time>
  )
}
