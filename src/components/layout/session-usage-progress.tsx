"use client"

import { useTranslations } from "next-intl"
import { formatContextWindowPercent } from "@/lib/context-window"

const RING_SIZE = 20
const RING_CENTER = RING_SIZE / 2
const RING_RADIUS = 8
const RING_STROKE_WIDTH = 2.5
const MAX_PERCENT = 100
const WARNING_PERCENT = 90

export function SessionUsageProgress({ value }: { value: number | null }) {
  const t = useTranslations("Folder.statusBar.tokens")
  const progress =
    value === null ? null : Math.min(MAX_PERCENT, Math.max(0, value))
  return (
    <svg
      viewBox={`0 0 ${RING_SIZE} ${RING_SIZE}`}
      className="size-5 shrink-0"
      role="progressbar"
      aria-label={t("contextWindowUsageAria")}
      aria-valuemin={0}
      aria-valuemax={MAX_PERCENT}
      aria-valuenow={progress ?? undefined}
      aria-valuetext={
        progress === null
          ? t("unreported")
          : formatContextWindowPercent(progress)
      }
      fill="none"
      strokeWidth={RING_STROKE_WIDTH}
    >
      <circle
        cx={RING_CENTER}
        cy={RING_CENTER}
        r={RING_RADIUS}
        className="stroke-muted-foreground/25"
        strokeDasharray={progress === null ? "2 2" : undefined}
      />
      {progress !== null && progress > 0 && (
        <circle
          cx={RING_CENTER}
          cy={RING_CENTER}
          r={RING_RADIUS}
          className={
            progress >= WARNING_PERCENT
              ? "stroke-amber-500"
              : "stroke-blue-500 dark:stroke-blue-400"
          }
          pathLength={MAX_PERCENT}
          strokeDasharray={`${progress} ${MAX_PERCENT}`}
          strokeLinecap="round"
          transform={`rotate(-90 ${RING_CENTER} ${RING_CENTER})`}
        />
      )}
    </svg>
  )
}
