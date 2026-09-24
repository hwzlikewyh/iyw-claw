"use client"

import { useEffect, useMemo, useState } from "react"
import { Check, ListFilter, X } from "lucide-react"
import { useTranslations } from "next-intl"
import {
  Popover,
  PopoverContent,
  PopoverTrigger,
} from "@/components/ui/popover"
import { Button } from "@/components/ui/button"
import { useSidebarViewOptions } from "@/contexts/sidebar-view-options-context"
import {
  STATUS_ORDER,
  type ConversationStatus,
  type DbConversationSummary,
} from "@/lib/types"
import { cn } from "@/lib/utils"

export interface SidebarFilters {
  status: ConversationStatus | "all"
  period: "all" | "today" | "week" | "month"
}

export const DEFAULT_SIDEBAR_FILTERS: SidebarFilters = {
  status: "all",
  period: "all",
}
const DAY_MS = 86_400_000
const PERIOD_DAYS = { week: 7, month: 30 } as const
const FILTER_REFRESH_MS = 60_000

export function useFilteredSidebarConversations(
  conversations: DbConversationSummary[],
  filters: SidebarFilters
) {
  const [now, setNow] = useState(Date.now)
  useEffect(() => {
    if (filters.period === "all") return
    const update = window.setTimeout(() => setNow(Date.now()), 0)
    const timer = window.setInterval(
      () => setNow(Date.now()),
      FILTER_REFRESH_MS
    )
    return () => {
      window.clearTimeout(update)
      window.clearInterval(timer)
    }
  }, [filters.period])
  return useMemo(
    () =>
      filters.status === "all" && filters.period === "all"
        ? conversations
        : conversations.filter((conversation) =>
            matchesSidebarFilters(conversation, filters, now)
          ),
    [conversations, filters, now]
  )
}

export function matchesSidebarFilters(
  conversation: DbConversationSummary,
  filters: SidebarFilters,
  now: number
) {
  if (filters.status !== "all" && conversation.status !== filters.status)
    return false
  if (filters.period === "all") return true
  const start = new Date(now)
  start.setHours(0, 0, 0, 0)
  const cutoff =
    filters.period === "today"
      ? start.getTime()
      : now - PERIOD_DAYS[filters.period] * DAY_MS
  const updated = Date.parse(conversation.updated_at)
  return Number.isFinite(updated) && updated >= cutoff
}

interface Props {
  value: SidebarFilters
  onChange: (next: SidebarFilters) => void
}

export function SidebarFilterControl({ value, onChange }: Props) {
  const t = useTranslations("SidebarDesign")
  const tManage = useTranslations("Folder.sidebar.manageConversations")
  const tSidebar = useTranslations("Folder.sidebar")
  const common = useTranslations("Folder.common")
  const { showCompleted, setShowCompleted } = useSidebarViewOptions()
  const [open, setOpen] = useState(false)
  const active = value.status !== "all" || value.period !== "all"
  return (
    <Popover open={open} onOpenChange={setOpen}>
      <PopoverTrigger asChild>
        <Button
          variant="ghost"
          size="icon"
          title={t("filter")}
          aria-label={t("filter")}
          className={cn(
            "relative size-7 rounded-md",
            active && "bg-primary/10 text-primary"
          )}
        >
          <ListFilter className="size-3.5" />
          {active && (
            <span className="absolute right-1 top-1 size-1 rounded-full bg-primary" />
          )}
        </Button>
      </PopoverTrigger>
      <PopoverContent
        align="end"
        className="w-80 max-w-[calc(100vw-1rem)] gap-0 rounded-lg border p-2"
      >
        <div className="flex items-center justify-between px-2 pb-2 text-xs font-medium">
          {t("filter")}
          <Button
            variant="ghost"
            size="icon"
            className="size-6"
            onClick={() => setOpen(false)}
            aria-label={common("close")}
          >
            <X className="size-3" />
          </Button>
        </div>
        <div className="grid grid-cols-2 divide-x">
          <StatusFilterOptions
            value={value.status}
            onChange={(status) => onChange({ ...value, status })}
            allLabel={tManage("statusFilterAll")}
          />
          <FilterOptions
            title={t("time")}
            value={value.period}
            onChange={(period) => onChange({ ...value, period })}
            options={(["all", "today", "week", "month"] as const).map(
              (period) => ({
                value: period,
                label: t(period === "all" ? "allTime" : period),
              })
            )}
          />
        </div>
        {value.status === "all" && (
          <label className="mt-2 flex items-center gap-2 border-t px-2 pt-3 text-xs">
            <input
              type="checkbox"
              checked={showCompleted}
              onChange={(event) => setShowCompleted(event.target.checked)}
              className="accent-primary"
            />
            {tSidebar("showCompleted")}
          </label>
        )}
        <Button
          variant="ghost"
          size="sm"
          disabled={!active}
          className="mt-2 justify-start text-xs"
          onClick={() => onChange(DEFAULT_SIDEBAR_FILTERS)}
        >
          {t("reset")}
        </Button>
      </PopoverContent>
    </Popover>
  )
}

function FilterOptions<T extends string>({
  title,
  value,
  options,
  onChange,
}: {
  title: string
  value: T
  options: { value: T; label: string }[]
  onChange: (value: T) => void
}) {
  return (
    <fieldset className="min-w-0 px-1">
      <legend className="px-2 pb-1 text-[0.6875rem] text-muted-foreground">
        {title}
      </legend>
      {options.map((option) => (
        <label
          key={option.value}
          className={cn(
            "relative flex min-h-8 cursor-pointer items-center gap-2 rounded px-2 text-xs hover:bg-accent has-[:focus-visible]:ring-2 has-[:focus-visible]:ring-ring",
            value === option.value && "bg-accent font-medium"
          )}
        >
          <input
            type="radio"
            className="sr-only"
            name={title}
            checked={value === option.value}
            onChange={() => onChange(option.value)}
          />
          <span className="min-w-0 flex-1">{option.label}</span>
          {value === option.value && (
            <Check className="size-3 shrink-0 text-primary" />
          )}
        </label>
      ))}
    </fieldset>
  )
}

function StatusFilterOptions({
  value,
  onChange,
  allLabel,
}: {
  value: SidebarFilters["status"]
  onChange: (value: SidebarFilters["status"]) => void
  allLabel: string
}) {
  const tStatus = useTranslations("Folder.statusLabels")
  const t = useTranslations("SidebarDesign")
  return (
    <FilterOptions
      title={t("status")}
      value={value}
      onChange={onChange}
      options={[
        { value: "all" as const, label: allLabel },
        ...STATUS_ORDER.map((status) => ({
          value: status,
          label: tStatus(status),
        })),
      ]}
    />
  )
}
