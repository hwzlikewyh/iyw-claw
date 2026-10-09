"use client"

import { useTranslations } from "next-intl"
import type { GrantLevel, PickerWindow } from "@/lib/computer/types"
import { cn } from "@/lib/utils"
import { Thumbnail } from "./thumbnail"
import { LevelControl } from "./level-control"

export function WindowTile({
  item: w,
  level,
  viaApp,
  viaScreen,
  pending,
  disabled,
  pictures,
  onLevel,
}: {
  item: PickerWindow
  level: GrantLevel
  viaApp: boolean
  viaScreen: boolean
  pending: GrantLevel | null
  disabled: boolean
  pictures: number
  onLevel: (next: GrantLevel) => void
}) {
  const t = useTranslations("ComputerUse.picker")
  const appName = w.appName || t("unnamedApp")
  return (
    <div
      className={cn(
        "flex flex-col overflow-hidden rounded-2xl border bg-card transition-[border-color,box-shadow]",
        level === "none" && "hover:border-foreground/20",
        level === "read" && "border-violet-500/70 ring-2 ring-violet-500/15",
        level === "control" && "border-red-500/70 ring-2 ring-red-500/15"
      )}
    >
      <div className="relative">
        <Thumbnail key={`${w.targetId}:${pictures}`} targetId={w.targetId} />
        {(w.minimized || w.hidden) && (
          <span className="absolute start-2 top-2 rounded-full bg-background/85 px-2 py-0.5 text-2xs text-muted-foreground shadow-sm backdrop-blur-sm">
            {w.minimized ? t("minimized") : t("hidden")}
          </span>
        )}
        {(viaApp || viaScreen) && (
          <span className="absolute end-2 top-2 rounded-full bg-background/85 px-2 py-0.5 text-2xs text-muted-foreground shadow-sm backdrop-blur-sm">
            {t(viaScreen ? "viaScreen" : "viaApp")}
          </span>
        )}
      </div>
      <div className="flex flex-1 flex-col gap-2 border-t px-2.5 pt-2 pb-2.5">
        <div className="min-w-0 flex-1">
          <p className="truncate text-xs font-medium" title={appName}>
            {appName}
          </p>
          <p
            className="truncate text-2xs text-muted-foreground"
            title={w.title}
          >
            {w.title || t("untitled")}
          </p>
        </div>
        <LevelControl
          label={t("levelLabel", { app: appName })}
          level={level}
          pending={pending}
          disabled={disabled || viaApp || viaScreen}
          onLevel={onLevel}
        />
      </div>
    </div>
  )
}
