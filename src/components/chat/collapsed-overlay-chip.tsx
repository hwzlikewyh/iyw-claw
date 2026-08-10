"use client"

/**
 * Collapsed chip for the inline-start conversation overlays (the message
 * navigator, the plan panel, and the sub-agent panel).
 *
 * Rests as a dimmed bullet — a flat start-side edge with a rounded end-side (a
 * "bullet" pointing toward where it expands: right in LTR, mirrored in RTL),
 * flush to the inline-start edge; minimal and unobtrusive so it doesn't crowd
 * the message area, and brightens to full opacity on hover/focus. Built from
 * logical properties (`rounded-s/e`, `pe`, an `rtl:`-flipped chevron) so it
 * mirrors cleanly under `dir="rtl"`. The summary is `display:none` at rest, so
 * the resting button is only the responsive icon cap (flat start-side, round
 * end-side, no way for text to leak); on hover or keyboard focus it switches
 * to `flex` and reveals the full pill (summary + chevron). Clicking it expands
 * the owning overlay. Shared so every chip in the stack stays pixel identical.
 *
 * `summary` is the visible text AND the button's accessible name (`aria-label`),
 * so what a screen reader / voice-control user gets matches what a sighted user
 * reads on hover (WCAG 2.5.3); `aria-expanded` conveys the collapsed disclosure
 * state. A 1px border traces the top, bottom, and rounded end — the flush
 * start edge stays open; keyboard focus adds an inset `ring`. The icon cap is
 * 28px on compact viewports and 32px from `sm` upward, giving these edge
 * controls a clearer target without crowding narrow conversations.
 */

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
}

export function CollapsedOverlayChip({
  icon,
  summary,
  onClick,
}: CollapsedOverlayChipProps) {
  return (
    <div className="pointer-events-none flex">
      <button
        type="button"
        aria-label={summary}
        aria-expanded={false}
        onClick={onClick}
        className={cn(
          "group/chip pointer-events-auto flex max-w-[min(22rem,calc(100vw-2rem))] items-center rounded-s-none rounded-e-full border-y border-e",
          "bg-secondary/70 text-secondary-foreground opacity-60 shadow-md transition-[background-color,opacity] duration-150 hover:bg-secondary hover:opacity-100 focus-visible:opacity-100",
          "cursor-pointer outline-none focus-visible:ring-[3px] focus-visible:ring-inset focus-visible:ring-ring/50"
        )}
      >
        {/* Fixed icon cap — the whole resting chip is just this bullet cap. */}
        <span className="grid size-7 shrink-0 place-items-center sm:size-8">
          {icon}
        </span>
        {/* Summary: hidden at rest (no width), revealed on hover/focus. */}
        <span className="hidden min-w-0 items-center gap-1 pe-3 text-sm font-medium group-hover/chip:flex group-focus-visible/chip:flex">
          <span className="truncate">{summary}</span>
          <ChevronRightIcon className="size-3.5 shrink-0 text-muted-foreground rtl:-scale-x-100" />
        </span>
      </button>
    </div>
  )
}
