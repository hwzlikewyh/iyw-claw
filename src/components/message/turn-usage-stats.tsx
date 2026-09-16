"use client"

import { Coins } from "lucide-react"
import { useLocale, useTranslations } from "next-intl"
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from "@/components/ui/tooltip"
import type { TurnUsage } from "@/lib/types"
import { UsagePointsRow, UsagePointsValue } from "./usage-points"

export function TurnUsageStats({ usage }: { usage: TurnUsage }) {
  const locale = useLocale()
  const t = useTranslations("Folder.chat.messageList")
  const pointsT = useTranslations("UsagePoints")
  const rows = [
    { key: "tokenInput", value: usage.input_tokens },
    { key: "tokenOutput", value: usage.output_tokens },
    { key: "tokenCacheRead", value: usage.cache_read_input_tokens },
    { key: "tokenCacheWrite", value: usage.cache_creation_input_tokens },
  ] as const
  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <button
          type="button"
          className="inline-flex h-6 cursor-default items-center gap-1 rounded-md px-1.5 text-muted-foreground hover:bg-muted hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
          aria-label={pointsT("turn")}
        >
          <Coins aria-hidden="true" className="h-3.5 w-3.5 shrink-0" />
          <UsagePointsValue points={usage.estimated_points} />
        </button>
      </TooltipTrigger>
      <TooltipContent side="top" className="min-w-48 max-w-xs">
        <div className="flex flex-col gap-0.5">
          {rows
            .filter((row) => row.value > 0 || row.key === "tokenInput")
            .map((row) => (
              <div key={row.key} className="flex justify-between gap-3">
                <span>{t(row.key)}</span>
                <span className="font-mono tabular-nums">
                  {row.value.toLocaleString(locale)}
                </span>
              </div>
            ))}
          <UsagePointsRow points={usage.estimated_points} scope="turn" />
          <span className="mt-1 text-[10px] opacity-70">
            {pointsT("estimateShort")}
          </span>
        </div>
      </TooltipContent>
    </Tooltip>
  )
}
