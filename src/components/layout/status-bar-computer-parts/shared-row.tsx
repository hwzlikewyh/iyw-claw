"use client"

import { useTranslations } from "next-intl"
import { AppWindow, ChevronDown, Monitor, ScreenShare } from "lucide-react"
import { Button } from "@/components/ui/button"
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu"
import type { GrantLevel } from "@/lib/computer/types"
import { cn } from "@/lib/utils"
import { AGENT_MARK } from "../status-bar-computer"

export function SharedRow({
  whole = false,
  screen = false,
  name,
  detail,
  level,
  onLevel,
}: {
  whole?: boolean
  screen?: boolean
  name: string
  detail?: string
  level: GrantLevel
  onLevel: (level: GrantLevel) => void
}) {
  const t = useTranslations("ComputerUse")
  const Icon = screen ? ScreenShare : whole ? AppWindow : Monitor
  return (
    <div className="flex items-center gap-2 px-2 py-1.5">
      <Icon className={cn("size-3.5 shrink-0", AGENT_MARK)} />
      <span className="min-w-0 flex-1">
        <span className="block truncate text-2xs font-medium">{name}</span>
        {detail && (
          <span className="block truncate text-3xs text-muted-foreground">
            {detail}
          </span>
        )}
      </span>
      <DropdownMenu>
        <DropdownMenuTrigger asChild>
          <Button
            size="xs"
            variant="ghost"
            aria-label={t("level.change")}
            className={cn(
              level === "control" && "text-red-600 dark:text-red-400"
            )}
          >
            {t(`level.${level === "control" ? "control" : "read"}`)}
            <ChevronDown className="size-3" />
          </Button>
        </DropdownMenuTrigger>
        <DropdownMenuContent align="end" className="min-w-48">
          {(["read", "control"] as const).map((option) => (
            <DropdownMenuItem
              key={option}
              disabled={level === option}
              onSelect={() => onLevel(option)}
            >
              {t(`level.${option}Long`)}
            </DropdownMenuItem>
          ))}
        </DropdownMenuContent>
      </DropdownMenu>
      <Button size="xs" variant="ghost" onClick={() => onLevel("none")}>
        {t("shared.stop")}
      </Button>
    </div>
  )
}
