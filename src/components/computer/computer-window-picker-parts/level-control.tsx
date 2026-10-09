"use client"

import { useTranslations } from "next-intl"
import { Loader2 } from "lucide-react"
import type { GrantLevel } from "@/lib/computer/types"
import { cn } from "@/lib/utils"
import { LEVELS } from "../computer-window-picker"

export function LevelControl({
  label,
  level,
  pending,
  disabled,
  onLevel,
}: {
  label: string
  level: GrantLevel
  pending: GrantLevel | null
  disabled: boolean
  onLevel: (next: GrantLevel) => void
}) {
  const t = useTranslations("ComputerUse.picker")
  const hint: Record<GrantLevel, string | undefined> = {
    none: level === "none" ? undefined : t("stopSharing"),
    read: t("shareRead"),
    control: t("shareControl"),
  }
  return (
    <div
      role="group"
      aria-label={label}
      className="grid grid-cols-3 gap-0.5 rounded-full bg-muted p-0.5"
    >
      {LEVELS.map(({ level: option, label }) => {
        const on = level === option
        return (
          <button
            key={option}
            type="button"
            aria-pressed={on}
            disabled={disabled}
            title={hint[option]}
            onClick={() => {
              if (!on) onLevel(option)
            }}
            className={cn(
              "flex h-6 min-w-0 items-center justify-center gap-1 rounded-full px-1.5 text-2xs font-medium text-muted-foreground transition-colors outline-none focus-visible:ring-2 focus-visible:ring-ring/50 disabled:cursor-default disabled:opacity-60",
              !on && "hover:bg-background/60 hover:text-foreground",
              on && "bg-background text-foreground shadow-sm",
              on &&
                option === "read" &&
                "bg-violet-600 text-white dark:bg-violet-500",
              on &&
                option === "control" &&
                "bg-red-600 text-white dark:bg-red-500"
            )}
          >
            {pending === option && (
              <Loader2 className="size-3 shrink-0 animate-spin" />
            )}
            <span className="truncate">{t(label)}</span>
          </button>
        )
      })}
    </div>
  )
}
