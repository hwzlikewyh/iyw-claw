"use client"

import { memo, useMemo, useState } from "react"
import { Check, ChevronDown, CircleDashed, CircleDot } from "lucide-react"
import { useTranslations } from "next-intl"
import type { PlanEntryInfo } from "@/lib/types"
import { cn } from "@/lib/utils"
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from "@/components/ui/collapsible"
import { PlanEntriesList } from "./plan-card"
import { readableStatusText } from "./live-turn-summary"

const MAX_VISIBLE_STEPS = 3

function visibleSteps(entries: PlanEntryInfo[]) {
  const active = entries.findIndex((entry) => entry.status === "in_progress")
  const pending = entries.findIndex((entry) => entry.status === "pending")
  const current =
    active >= 0 ? active : pending >= 0 ? pending : entries.length - 1
  const start = Math.max(
    0,
    Math.min(current - 1, entries.length - MAX_VISIBLE_STEPS)
  )
  return entries
    .slice(start, start + MAX_VISIBLE_STEPS)
    .map((entry, index) => ({ entry, index: start + index }))
}

function Step({ entry, index }: { entry: PlanEntryInfo; index: number }) {
  const t = useTranslations("Folder.chat.liveTurnStats")
  const title =
    readableStatusText(entry.content) ?? t("planStep", { number: index + 1 })
  const completed = entry.status === "completed"
  const active = entry.status === "in_progress"
  const Icon = completed ? Check : active ? CircleDot : CircleDashed
  return (
    <span
      className={cn(
        "flex min-w-0 flex-1 items-center gap-1.5",
        completed && "text-emerald-700 dark:text-emerald-300",
        active && "font-medium text-foreground"
      )}
    >
      <Icon aria-hidden="true" className="size-3 shrink-0" />
      <span
        className="line-clamp-2 break-words leading-4 [overflow-wrap:anywhere]"
        title={title}
      >
        {title}
      </span>
    </span>
  )
}

function PlanTrigger({
  steps,
  completed,
  total,
  open,
}: {
  steps: ReturnType<typeof visibleSteps>
  completed: number
  total: number
  open: boolean
}) {
  const t = useTranslations("Folder.chat.agentPlanOverlay")
  return (
    <CollapsibleTrigger
      className="flex min-h-8 w-full min-w-0 items-center gap-3 rounded text-left text-[11px] text-muted-foreground outline-none hover:text-foreground focus-visible:ring-2 focus-visible:ring-ring"
      aria-label={t("title")}
    >
      <span className="flex min-w-0 max-w-[27rem] flex-1 items-center gap-2">
        {steps.map(({ entry, index }, position) => (
          <span key={index} className="flex min-w-0 flex-1 items-center gap-2">
            {position > 0 && (
              <span
                aria-hidden="true"
                className="h-px w-3 shrink-0 bg-border"
              />
            )}
            <Step entry={entry} index={index} />
          </span>
        ))}
      </span>
      <span className="ms-auto inline-flex shrink-0 items-center gap-2 tabular-nums">
        {completed}/{total}
        <ChevronDown
          aria-hidden="true"
          className={cn(
            "size-3 transition-transform motion-reduce:transition-none",
            open && "rotate-180"
          )}
        />
      </span>
    </CollapsibleTrigger>
  )
}

export const LiveTurnPlan = memo(function LiveTurnPlan({
  entries,
  isStreaming,
}: {
  entries: PlanEntryInfo[]
  isStreaming: boolean
}) {
  const [open, setOpen] = useState(false)
  const steps = useMemo(() => visibleSteps(entries), [entries])
  const completed = useMemo(
    () => entries.filter((entry) => entry.status === "completed").length,
    [entries]
  )
  if (entries.length === 0) return null
  return (
    <Collapsible
      open={open}
      onOpenChange={setOpen}
      className="mt-2 min-w-0 @[28rem]/turnstats:ms-[49px]"
    >
      <PlanTrigger
        steps={steps}
        completed={completed}
        total={entries.length}
        open={open}
      />
      <CollapsibleContent>
        <div className="mt-1 max-h-44 overflow-y-auto border-t py-2">
          <PlanEntriesList entries={entries} isStreaming={isStreaming} />
        </div>
      </CollapsibleContent>
    </Collapsible>
  )
})
