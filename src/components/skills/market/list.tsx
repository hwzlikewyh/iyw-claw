"use client"

import { RotateCcw, SearchX } from "lucide-react"
import { useTranslations } from "next-intl"
import { Button } from "@/components/ui/button"
import { useMarketPageFocus } from "./use-market-page-focus"
import { Skeleton } from "@/components/ui/skeleton"
import { SkillCard } from "@/components/skills/market/skill-card"
import type { SkillMarketV2Item } from "@/lib/skill-market"
import {
  MarketPagination,
  type MarketPaginationProps,
} from "./market-pagination"

function ListState({
  kind,
  error,
  onRetry,
}: {
  kind: "loading" | "empty" | "error"
  error?: string | null
  onRetry: () => void
}) {
  const t = useTranslations("SkillMarketV2")
  if (kind === "loading") {
    return (
      <div
        data-market-grid
        className="grid grid-cols-1 gap-4 @min-[560px]:grid-cols-2 @min-[960px]:grid-cols-3"
      >
        {Array.from({ length: 6 }, (_, index) => (
          <Skeleton
            data-market-skeleton
            key={index}
            className="h-52 rounded-lg"
          />
        ))}
      </div>
    )
  }
  return (
    <div
      data-market-result-state
      role={kind === "error" ? "alert" : "status"}
      className="flex h-full min-h-64 flex-col items-center justify-center gap-2 p-6 text-center"
    >
      {kind === "empty" ? (
        <SearchX className="size-6 text-muted-foreground" aria-hidden="true" />
      ) : null}
      <p className="text-sm font-medium">
        {t(kind === "empty" ? "list.empty" : "list.error")}
      </p>
      {error ? <p className="text-xs text-muted-foreground">{error}</p> : null}
      {kind === "error" ? (
        <Button size="sm" variant="outline" onClick={onRetry}>
          <RotateCcw className="size-3.5" aria-hidden="true" />
          {t("list.retry")}
        </Button>
      ) : (
        <p className="text-xs text-muted-foreground">{t("list.emptyHint")}</p>
      )}
    </div>
  )
}

export interface SkillMarketListProps extends Omit<
  MarketPaginationProps,
  "loading"
> {
  items: SkillMarketV2Item[]
  selectedId: string | null
  loading: boolean
  error: string | null
  onSelect: (item: SkillMarketV2Item) => void
  onPrimaryAction: (item: SkillMarketV2Item) => void
  onRetry: () => void
}

export function SkillMarketList(props: SkillMarketListProps) {
  const root = useMarketPageFocus(props)
  return (
    <div ref={root} className="@container min-w-0" aria-busy={props.loading}>
      <section>
        <SkillPageContent {...props} />
      </section>
      {!props.error && <MarketPagination {...props} />}
    </div>
  )
}

function SkillPageContent(props: SkillMarketListProps) {
  if (props.loading) return <ListState kind="loading" onRetry={props.onRetry} />
  if (props.error)
    return (
      <ListState kind="error" error={props.error} onRetry={props.onRetry} />
    )
  if (!props.items.length)
    return <ListState kind="empty" onRetry={props.onRetry} />
  return (
    <div
      data-market-grid
      className="grid grid-cols-1 gap-4 @min-[560px]:grid-cols-2 @min-[960px]:grid-cols-3"
    >
      {props.items.map((item) => (
        <SkillCard
          key={item.id}
          item={item}
          selected={props.selectedId === item.id}
          onSelect={props.onSelect}
          onPrimaryAction={props.onPrimaryAction}
        />
      ))}
    </div>
  )
}
