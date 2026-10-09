"use client"

import { useTranslations } from "next-intl"
import { ScreenShare } from "lucide-react"
import type { GrantLevel } from "@/lib/computer/types"
import { cn } from "@/lib/utils"
import { LevelControl } from "./level-control"

export function ScreenCard({
  level,
  pending,
  disabled,
  onLevel,
}: {
  level: GrantLevel
  pending: GrantLevel | null
  disabled: boolean
  onLevel: (next: GrantLevel) => void
}) {
  const t = useTranslations("ComputerUse.picker")
  return (
    <section
      aria-label={t("screenTitle")}
      className={cn(
        "flex flex-wrap items-center gap-x-3 gap-y-2 rounded-2xl border bg-card px-3 py-2.5 transition-[border-color,box-shadow]",
        level === "read" && "border-violet-500/70 ring-2 ring-violet-500/15",
        level === "control" && "border-red-500/70 ring-2 ring-red-500/15"
      )}
    >
      <ScreenShare className="size-4 shrink-0 text-muted-foreground" />
      <div className="min-w-0 flex-1">
        <p className="text-xs font-medium">{t("screenTitle")}</p>
        <p className="text-2xs leading-snug text-muted-foreground">
          {t("screenHint")}
        </p>
      </div>
      <div className="w-48">
        <LevelControl
          label={t("screenLevelLabel")}
          level={level}
          pending={pending}
          disabled={disabled}
          onLevel={onLevel}
        />
      </div>
    </section>
  )
}
