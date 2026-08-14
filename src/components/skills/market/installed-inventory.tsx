"use client"

import { useMemo, useState } from "react"
import { Loader2 } from "lucide-react"
import { useTranslations } from "next-intl"
import { Button } from "@/components/ui/button"
import { cn } from "@/lib/utils"
import {
  InventoryDetail,
  InventoryRow,
  type InventoryActions,
} from "@/components/skills/market/installed-inventory-detail"
import type {
  LogicalSkillInventoryItem,
  SkillInventorySnapshot,
} from "@/lib/types"

interface InstalledInventoryProps extends InventoryActions {
  snapshot: SkillInventorySnapshot | null
  query: string
  loading: boolean
  error: string | null
  busyKey: string | null
  onRetry: () => void
}

function inventoryKey(skill: LogicalSkillInventoryItem) {
  return `${skill.scope}:${skill.skillId}`
}

export function InstalledInventoryView(props: InstalledInventoryProps) {
  const t = useTranslations("SkillMarketV2.inventory")
  const [selectedKey, setSelectedKey] = useState<string | null>(null)
  const skills = useMemo(() => {
    const query = props.query.trim().toLocaleLowerCase()
    const all = props.snapshot?.skills ?? []
    if (!query) return all
    return all.filter((skill) =>
      `${skill.name} ${skill.skillId} ${skill.description ?? ""}`
        .toLocaleLowerCase()
        .includes(query)
    )
  }, [props.query, props.snapshot?.skills])
  const selected =
    skills.find((skill) => inventoryKey(skill) === selectedKey) ?? skills[0]
  const budget = [...(props.snapshot?.descriptionBudgets ?? [])].sort(
    (left, right) => right.usedChars - left.usedChars
  )[0]
  if (props.loading && !props.snapshot) {
    return (
      <div className="flex h-full items-center justify-center">
        <Loader2 className="size-5 animate-spin" />
      </div>
    )
  }
  if (props.error && !props.snapshot) {
    return (
      <div className="flex h-full flex-col items-center justify-center gap-3 px-5 text-center">
        <p className="text-sm">{t("loadFailed")}</p>
        <p className="text-xs text-muted-foreground">{props.error}</p>
        <Button size="sm" variant="outline" onClick={props.onRetry}>
          {t("retry")}
        </Button>
      </div>
    )
  }
  if (!skills.length) {
    return (
      <div className="flex h-full items-center justify-center text-sm text-muted-foreground">
        {t("empty")}
      </div>
    )
  }
  return (
    <div className="grid h-full min-h-0 grid-cols-1 sm:grid-cols-[minmax(0,1fr)_minmax(280px,36%)]">
      <div className="min-h-0 overflow-y-auto border-r">
        {budget ? (
          <div className="border-b bg-muted/20 px-4 py-2 text-[11px] text-muted-foreground">
            <span
              className={cn(
                budget.overSoftLimit && "text-amber-700 dark:text-amber-300"
              )}
            >
              {t("budget", {
                used: budget.usedChars,
                limit: budget.softLimitChars,
                count: budget.skillCount,
              })}
            </span>
          </div>
        ) : null}
        {skills.map((skill) => (
          <InventoryRow
            key={inventoryKey(skill)}
            skill={skill}
            selected={skill === selected}
            onSelect={() => setSelectedKey(inventoryKey(skill))}
          />
        ))}
      </div>
      {selected ? (
        <InventoryDetail
          skill={selected}
          busyKey={props.busyKey}
          onToggle={props.onToggle}
          onTakeOver={props.onTakeOver}
          onReconcile={props.onReconcile}
        />
      ) : null}
    </div>
  )
}
