"use client"

import { useState } from "react"
import { Coins, Gauge } from "lucide-react"
import { useLocale, useTranslations } from "next-intl"
import { formatTokenThousands } from "@/lib/token-format"
import { formatContextWindowPercent } from "@/lib/context-window"
import { cn } from "@/lib/utils"
import { UsagePointsValue } from "@/components/message/usage-points"
import {
  Popover,
  PopoverContent,
  PopoverTrigger,
} from "@/components/ui/popover"
import { SessionUsageContent } from "./session-usage-content"
import {
  useSessionUsage,
  type SessionUsageSourceProps,
} from "./use-session-usage"

interface SessionUsageButtonProps extends SessionUsageSourceProps {
  variant?: "status" | "chip"
  className?: string
  popoverSide?: "top" | "right" | "bottom" | "left"
  popoverSideOffset?: number
  popoverAvoidCollisions?: boolean
  stopPropagation?: boolean
  showIcon?: boolean
}

function UsageTrigger({
  props,
  data,
}: {
  props: SessionUsageButtonProps
  data: ReturnType<typeof useSessionUsage>["data"]
}) {
  const t = useTranslations("Folder.statusBar.tokens")
  const locale = useLocale()
  const Icon = data.contextPercent === null ? Coins : Gauge
  const stop = (event: React.SyntheticEvent) => {
    if (props.stopPropagation) event.stopPropagation()
  }
  return (
    <PopoverTrigger asChild>
      <button
        type="button"
        aria-label={t("details")}
        title={t("details")}
        onMouseDown={stop}
        onClick={stop}
        onDoubleClick={stop}
        className={cn(
          "inline-flex shrink-0 items-center gap-1 tabular-nums transition-colors",
          props.variant === "chip"
            ? "h-[1.125rem] rounded-md border border-sidebar-border/60 bg-sidebar-accent/55 px-1.5 text-[0.625rem] font-medium text-muted-foreground hover:bg-sidebar-accent hover:text-sidebar-foreground"
            : "hover:text-foreground",
          props.className
        )}
      >
        {props.showIcon !== false && <Icon className="size-3 shrink-0" />}
        <span>
          {data.contextPercent !== null
            ? formatContextWindowPercent(data.contextPercent)
            : data.total !== null
              ? formatTokenThousands(data.total, locale)
              : "--"}
        </span>
        {data.points !== null && (
          <span className="ms-1 border-s border-current/20 ps-1.5">
            <UsagePointsValue points={data.points} />
          </span>
        )}
      </button>
    </PopoverTrigger>
  )
}

function SessionUsageButton(props: SessionUsageButtonProps) {
  const [opened, setOpened] = useState(false)
  const { data, refresh, visible } = useSessionUsage(props, opened)
  if (!visible) return null
  return (
    <Popover
      open={opened}
      onOpenChange={(open) => {
        setOpened(open)
        if (open) refresh()
      }}
    >
      <UsageTrigger props={props} data={data} />
      <PopoverContent
        side={props.popoverSide ?? "top"}
        sideOffset={props.popoverSideOffset}
        align="end"
        avoidCollisions={props.popoverAvoidCollisions ?? true}
        className="max-h-[var(--radix-popover-content-available-height)] w-80 max-w-[calc(100vw-2rem)] gap-2 overflow-y-auto rounded-lg p-3 text-xs"
      >
        <SessionUsageContent data={data} />
      </PopoverContent>
    </Popover>
  )
}

export function StatusBarTokens(props: SessionUsageSourceProps) {
  return <SessionUsageButton {...props} variant="status" />
}

export function SessionUsageChip({
  popoverSide = "bottom",
  popoverSideOffset = 4,
  popoverAvoidCollisions = true,
  stopPropagation = true,
  ...props
}: Omit<SessionUsageButtonProps, "variant">) {
  return (
    <SessionUsageButton
      {...props}
      variant="chip"
      popoverSide={popoverSide}
      popoverSideOffset={popoverSideOffset}
      popoverAvoidCollisions={popoverAvoidCollisions}
      stopPropagation={stopPropagation}
    />
  )
}
