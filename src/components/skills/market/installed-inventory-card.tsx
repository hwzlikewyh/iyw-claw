"use client"

import { AlertTriangle, Bot, Copy, Package, ArrowUpRight } from "lucide-react"
import { useTranslations } from "next-intl"
import { Badge } from "@/components/ui/badge"
import { marketCardClass } from "@/components/skills/market/skill-card"
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
    <article className={marketCardClass(selected)}>
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
        <span className="mt-2.5 flex h-5 shrink-0 items-center gap-1.5 overflow-hidden">
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
