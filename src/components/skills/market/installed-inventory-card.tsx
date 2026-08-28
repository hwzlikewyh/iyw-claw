"use client"

import { AlertTriangle, Bot, Copy, Package, ArrowUpRight } from "lucide-react"
import { useTranslations } from "next-intl"
import { Badge } from "@/components/ui/badge"
import { cn } from "@/lib/utils"
import type {
  LogicalSkillInventoryItem,
  SkillInventoryStatus,
} from "@/lib/types"

function StatusIcon({ status }: { status: SkillInventoryStatus }) {
  if (
    status === "conflict" ||
    status === "blocked" ||
    status === "unreadable"
  ) {
    return <AlertTriangle className="size-4" />
  }
  if (status === "duplicate") return <Copy className="size-4" />
  if (status === "agent_builtin") return <Bot className="size-4" />
  return <Package className="size-4" />
}

export function InstalledInventoryCard({
  skill,
  selected,
  onSelect,
}: {
  skill: LogicalSkillInventoryItem
  selected: boolean
  onSelect: () => void
}) {
  const t = useTranslations("SkillMarketV2.inventory")
  const enabled = skill.agentStates.filter(
    (state) => state.actualEnabled
  ).length
  return (
    <article
      className={cn(
        "group flex h-[11.75rem] min-w-0 flex-col overflow-hidden rounded-lg border bg-background p-3.5 transition-[border-color,box-shadow,transform]",
        selected
          ? "border-foreground/35 shadow-[inset_3px_0_0_hsl(var(--foreground))]"
          : "hover:-translate-y-0.5 hover:border-foreground/20 hover:shadow-[0_8px_22px_rgba(15,23,42,0.055)]"
      )}
    >
      <button
        type="button"
        className="flex min-h-0 min-w-0 flex-1 flex-col overflow-hidden text-left outline-none focus-visible:ring-2 focus-visible:ring-ring/50"
        onClick={onSelect}
        aria-current={selected ? "true" : undefined}
        aria-label={t("openDetail", { name: skill.name })}
      >
        <span className="flex min-w-0 items-start gap-3">
          <span className="flex size-10 shrink-0 items-center justify-center rounded-md border bg-muted/35">
            <StatusIcon status={skill.status} />
          </span>
          <span className="min-w-0 flex-1">
            <span className="block truncate text-sm font-semibold">
              {skill.name}
            </span>
            <span className="mt-0.5 block truncate text-[10px] text-muted-foreground">
              {skill.skillId}
            </span>
          </span>
          <ArrowUpRight
            className="size-3.5 shrink-0 text-muted-foreground/70 transition-transform group-hover:translate-x-0.5 group-hover:-translate-y-0.5"
            aria-hidden="true"
          />
        </span>
        <span className="mt-2.5 flex h-5 items-center gap-1.5 overflow-hidden">
          {skill.localOnly ? (
            <Badge variant="secondary">{t("localOnly")}</Badge>
          ) : null}
        </span>
        <span className="mt-2 line-clamp-2 h-10 shrink-0 break-words text-xs leading-5 text-muted-foreground [overflow-wrap:anywhere]">
          {skill.description || t("noDescription")}
        </span>
      </button>
      <div className="mt-2.5 flex min-w-0 shrink-0 items-center border-t pt-2.5 text-[10px] text-muted-foreground">
        <span className="truncate">
          {t(`scope.${skill.scope}`)} ·{" "}
          {t("agentCount", { enabled, total: skill.agentStates.length })}
        </span>
      </div>
    </article>
  )
}
