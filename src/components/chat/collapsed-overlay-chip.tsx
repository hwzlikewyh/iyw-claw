"use client"

/** 入口图标固定在所属边缘，悬停或键盘聚焦时向会话内侧展开名称。 */

import type { ReactNode } from "react"
import { ChevronRightIcon } from "lucide-react"

import { cn } from "@/lib/utils"

interface CollapsedOverlayChipProps {
  /** Leading icon, shown alone when resting. Pass responsive sizing. */
  icon: ReactNode
  /** Summary text revealed on hover/focus, e.g. "子智能体 3" / "计划 2/5". Also
   *  the button's accessible name. */
  summary: string
  /** Expands the owning overlay into its full card. */
  onClick: () => void
  edge?: "start" | "end"
  disabled?: boolean
}

export function CollapsedOverlayChip({
  icon,
  summary,
  onClick,
  edge = "start",
  disabled,
}: CollapsedOverlayChipProps) {
  return (
    <div className="pointer-events-none flex max-w-full shrink-0">
      <button
        type="button"
        aria-label={summary}
        aria-expanded={false}
        disabled={disabled}
        onClick={onClick}
        className={cn(
          "group/chip pointer-events-auto flex min-w-0 max-w-[min(22rem,calc(100vw-2rem))] items-center border-y",
          edge === "end"
            ? "flex-row-reverse rounded-s-full border-s"
            : "rounded-e-full border-e opacity-60",
          "bg-secondary/70 text-secondary-foreground shadow-md transition-[background-color,opacity] duration-150 hover:bg-secondary hover:opacity-100 focus-visible:opacity-100",
          "cursor-pointer outline-none focus-visible:ring-[3px] focus-visible:ring-inset focus-visible:ring-ring/50 disabled:cursor-default disabled:opacity-50"
        )}
      >
        {/* Fixed icon cap — the whole resting chip is just this bullet cap. */}
        <span className="grid size-7 shrink-0 place-items-center sm:size-8">
          {icon}
        </span>
        {/* Summary: hidden at rest (no width), revealed on hover/focus. */}
        <span
          className={cn(
            "hidden min-w-0 items-center gap-1 text-sm font-medium group-hover/chip:flex group-focus-visible/chip:flex",
            edge === "end" ? "flex-row-reverse ps-3" : "pe-3"
          )}
        >
          <span className="truncate">{summary}</span>
          <ChevronRightIcon
            className={cn(
              "size-3.5 shrink-0 text-muted-foreground",
              edge === "end" ? "ltr:-scale-x-100" : "rtl:-scale-x-100"
            )}
          />
        </span>
      </button>
    </div>
  )
}
