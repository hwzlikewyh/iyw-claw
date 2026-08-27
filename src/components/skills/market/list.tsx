"use client"

import { Loader2, RotateCcw, SearchX } from "lucide-react"
import { useTranslations } from "next-intl"
import { Button } from "@/components/ui/button"
import { ScrollArea } from "@/components/ui/scroll-area"
import { Skeleton } from "@/components/ui/skeleton"
import { SkillCard } from "@/components/skills/market/skill-card"
import type { SkillMarketV2Item } from "@/lib/skill-market"

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
      <div className="grid grid-cols-[repeat(auto-fill,minmax(min(100%,15rem),1fr))] gap-3 p-4 sm:p-5">
        {Array.from({ length: 6 }, (_, index) => (
          <Skeleton key={index} className="h-[11.75rem] rounded-lg" />
        ))}
      </div>
    )
  }
  return (
    <div className="flex h-full min-h-64 flex-col items-center justify-center gap-2 p-6 text-center">
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

export interface SkillMarketListProps {
  items: SkillMarketV2Item[]
  selectedId: string | null
  loading: boolean
  error: string | null
  total: number
  nextCursor: string | null
  onSelect: (item: SkillMarketV2Item) => void
  onPrimaryAction: (item: SkillMarketV2Item) => void
  onLoadMore: () => void
  onRetry: () => void
}

export function SkillMarketList(props: SkillMarketListProps) {
  const t = useTranslations("SkillMarketV2")
  if (props.loading && !props.items.length) {
    return <ListState kind="loading" onRetry={props.onRetry} />
  }
  if (props.error && !props.items.length) {
    return (
      <ListState kind="error" error={props.error} onRetry={props.onRetry} />
    )
  }
  if (!props.items.length) {
    return <ListState kind="empty" onRetry={props.onRetry} />
  }
  return (
    <div className="flex h-full min-h-0 flex-col">
      <div className="flex h-8 shrink-0 items-end px-4 pb-1 text-[10px] text-muted-foreground sm:px-5">
        {t("list.count", { count: props.total })}
        {props.loading ? (
          <Loader2 className="ml-2 size-3 animate-spin" />
        ) : null}
      </div>
      <ScrollArea className="min-h-0 flex-1">
        <div className="grid grid-cols-[repeat(auto-fill,minmax(min(100%,15rem),1fr))] gap-3 p-4 pt-2 sm:p-5 sm:pt-2">
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
        {props.nextCursor ? (
          <div className="px-4 pb-4">
            <Button
              size="sm"
              variant="outline"
              className="w-full"
              disabled={props.loading}
              onClick={props.onLoadMore}
            >
              {t("list.loadMore")}
            </Button>
          </div>
        ) : null}
      </ScrollArea>
    </div>
  )
}
