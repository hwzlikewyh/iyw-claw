"use client"

import { useId, useRef, useState, useEffect } from "react"
import { ChevronDown, Coins, X } from "lucide-react"
import { useLocale, useTranslations } from "next-intl"
import {
  Popover,
  PopoverContent,
  PopoverTrigger,
} from "@/components/ui/popover"
import type { TurnUsage } from "@/lib/types"
import { UsagePointsValue } from "./usage-points"

function useUsagePopover() {
  const [open, setOpen] = useState(false)
  const pinnedRef = useRef(false)
  const restoreFocusRef = useRef(false)
  const timerRef = useRef<ReturnType<typeof setTimeout> | null>(null)
  const clearTimer = () => {
    if (timerRef.current) clearTimeout(timerRef.current)
  }
  const closeLater = () => {
    clearTimer()
    if (!pinnedRef.current)
      timerRef.current = setTimeout(() => setOpen(false), 160)
  }
  useEffect(
    () => () => {
      if (timerRef.current) clearTimeout(timerRef.current)
    },
    []
  )

  return {
    open,
    setOpen,
    pinnedRef,
    restoreFocusRef,
    timerRef,
    clearTimer,
    closeLater,
  }
}

type UsagePopover = ReturnType<typeof useUsagePopover>
interface UsagePopoverProps {
  usage?: TurnUsage | null
  points?: number | null
  live?: boolean
  controller: UsagePopover
  titleId: string
}

function UsageTrigger({ usage, controller, points, live }: UsagePopoverProps) {
  const t = useTranslations("Folder.chat.messageList")
  const {
    open,
    pinnedRef,
    restoreFocusRef,
    timerRef,
    clearTimer,
    closeLater,
    setOpen,
  } = controller
  return (
    <PopoverTrigger asChild>
      <button
        type="button"
        className="inline-flex min-h-8 items-center gap-1.5 rounded-md border border-border/60 bg-muted/20 px-2 text-xs text-muted-foreground hover:bg-muted hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring max-sm:min-h-10"
        title={t("turnUsage")}
        onPointerEnter={(event) => {
          clearTimer()
          if (event.pointerType !== "touch")
            timerRef.current = setTimeout(() => setOpen(true), 200)
        }}
        onPointerLeave={closeLater}
        onClick={(event) => {
          if (open && !pinnedRef.current) {
            event.preventDefault()
            pinnedRef.current = true
            restoreFocusRef.current = true
          }
        }}
      >
        <Coins aria-hidden="true" className="h-3.5 w-3.5 shrink-0" />
        <UsagePointsValue
          points={points ?? usage?.confirmed_points?.amount}
          consumption={usage?.confirmed_points}
          estimate={usage?.estimated_points}
          live={live}
          fixedDecimals
        />
        <ChevronDown aria-hidden="true" className="size-3 shrink-0" />
      </button>
    </PopoverTrigger>
  )
}

function UsageDetails({
  usage,
  points,
  live,
}: Pick<UsagePopoverProps, "usage" | "points" | "live">) {
  const t = useTranslations("Folder.chat.messageList")
  const locale = useLocale()
  const rows = usage
    ? ([
        { key: "tokenInput", value: usage.input_tokens },
        { key: "tokenOutput", value: usage.output_tokens },
        { key: "tokenCacheRead", value: usage.cache_read_input_tokens },
        { key: "tokenCacheWrite", value: usage.cache_creation_input_tokens },
      ] as const)
    : []
  return (
    <>
      <div className="mb-4 border-b pb-4">
        <div className="mb-1 text-xs text-muted-foreground">
          {t("turnPoints")}
        </div>
        <div className="text-2xl font-medium tabular-nums">
          <UsagePointsValue
            points={points ?? usage?.confirmed_points?.amount}
            consumption={usage?.confirmed_points}
            estimate={usage?.estimated_points}
            live={live}
            fixedDecimals
          />
        </div>
      </div>
      <div className="mb-3 flex justify-between text-xs text-muted-foreground">
        <span>{t("tokenDetails")}</span>
        <span>tokens</span>
      </div>
      <dl className="flex flex-col gap-3 text-xs">
        {rows
          .filter((row) => row.value > 0 || row.key === "tokenInput")
          .map((row) => (
            <div key={row.key} className="flex justify-between gap-3">
              <dt className="text-muted-foreground">{t(row.key)}</dt>
              <dd className="text-end font-medium tabular-nums">
                {row.value.toLocaleString(locale)}
              </dd>
            </div>
          ))}
      </dl>
    </>
  )
}

function UsagePanel(props: UsagePopoverProps) {
  const { usage, controller, titleId, points, live } = props
  const t = useTranslations("Folder.chat.messageList")
  const { pinnedRef, restoreFocusRef, clearTimer, closeLater, setOpen } =
    controller
  return (
    <PopoverContent
      side="top"
      align="end"
      sideOffset={8}
      aria-labelledby={titleId}
      className="w-72 max-w-[calc(100vw-1.5rem)] gap-0 rounded-xl border bg-popover p-4 text-popover-foreground shadow-lg ring-0 motion-reduce:animate-none"
      onOpenAutoFocus={(event) => {
        restoreFocusRef.current = pinnedRef.current
        if (!pinnedRef.current) event.preventDefault()
      }}
      onCloseAutoFocus={(event) => {
        if (!restoreFocusRef.current) event.preventDefault()
      }}
      onPointerEnter={clearTimer}
      onPointerLeave={closeLater}
      onFocus={() => {
        pinnedRef.current = true
        clearTimer()
      }}
    >
      <div className="mb-4 flex items-center justify-between gap-3">
        <h2 id={titleId} className="text-sm font-semibold">
          {t("turnUsage")}
        </h2>
        <button
          type="button"
          aria-label={t("closeDetails")}
          className="rounded-md p-1.5 text-muted-foreground hover:bg-muted focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
          onClick={() => {
            clearTimer()
            pinnedRef.current = false
            setOpen(false)
          }}
        >
          <X className="size-3.5" />
        </button>
      </div>

      <UsageDetails usage={usage} points={points} live={live} />
    </PopoverContent>
  )
}

export function TurnUsageStats({
  usage,
  points,
  live = false,
}: {
  usage?: TurnUsage | null
  points?: number | null
  live?: boolean
}) {
  const titleId = useId()
  const controller = useUsagePopover()
  const { pinnedRef, clearTimer, setOpen } = controller
  return (
    <Popover
      open={controller.open}
      onOpenChange={(next) => {
        clearTimer()
        pinnedRef.current = next
        setOpen(next)
      }}
    >
      <UsageTrigger
        usage={usage}
        points={points}
        live={live}
        controller={controller}
        titleId={titleId}
      />
      <UsagePanel
        usage={usage}
        points={points}
        live={live}
        controller={controller}
        titleId={titleId}
      />
    </Popover>
  )
}
