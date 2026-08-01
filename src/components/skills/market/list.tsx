"use client"

import { useCallback, useRef, useState } from "react"
import { Loader2, Package, RotateCcw, SearchX } from "lucide-react"
import { useTranslations } from "next-intl"
import { Avatar, AvatarFallback, AvatarImage } from "@/components/ui/avatar"
import { Button } from "@/components/ui/button"
import { ScrollArea } from "@/components/ui/scroll-area"
import { Skeleton } from "@/components/ui/skeleton"
import { Virtualizer, type VirtualizerHandle } from "virtua"
import { MarketBadgeGroup } from "@/components/skills/market/badges"
import {
  audienceBadgeInfo,
  compatibilityBadgeInfo,
  distributionBadgeInfo,
  installStateBadgeInfo,
  primaryInstallAction,
  type MarketBadgeInfo,
  type SkillMarketV2Item,
} from "@/lib/skill-market"
import { cn } from "@/lib/utils"

const ROW_HEIGHT = 88

function rowBadges(item: SkillMarketV2Item): MarketBadgeInfo[] {
  const badges: MarketBadgeInfo[] = [
    audienceBadgeInfo(item.audience),
    distributionBadgeInfo(item.distributionPolicy),
  ]
  if (item.installState === "installed" || item.installState === "update_available") {
    badges.push(installStateBadgeInfo(item.installState))
  }
  if (item.compatibility !== "compatible") {
    badges.push(compatibilityBadgeInfo(item.compatibility))
  }
  return badges
}

function MarketRow({
  item,
  selected,
  onSelect,
  onPrimaryAction,
}: {
  item: SkillMarketV2Item
  selected: boolean
  onSelect: (item: SkillMarketV2Item) => void
  onPrimaryAction: (item: SkillMarketV2Item) => void
}) {
  const t = useTranslations("SkillMarketV2")
  const action = primaryInstallAction(item.installState, item.compatibility)
  const artifactReady = item.currentVersion.status === "ready"
  const primaryKey = !artifactReady
    ? item.currentVersion.status === "artifact_pending"
      ? "waitingArtifact"
      : "buildFailed"
    : action
  return (
    <div
      className={cn(
        "flex h-[88px] items-center gap-3 border-b px-3",
        selected ? "bg-primary/5" : "hover:bg-muted/30"
      )}
    >
      <Avatar className="size-9 shrink-0 rounded-md">
        {item.iconUrl ? (
          <AvatarImage
            className="rounded-md"
            src={item.iconUrl}
            alt=""
            loading="lazy"
          />
        ) : null}
        <AvatarFallback className="rounded-md">
          <Package className="size-4" aria-hidden="true" />
        </AvatarFallback>
      </Avatar>
      <button
        type="button"
        data-market-row={item.id}
        onClick={() => onSelect(item)}
        aria-current={selected ? "true" : undefined}
        aria-label={t("a11y.openDetail", { name: item.displayName })}
        className="min-w-0 flex-1 rounded-md text-left outline-none focus-visible:ring-2 focus-visible:ring-ring/50"
      >
        <span className="block truncate text-sm font-medium">
          {item.displayName}
        </span>
        <span className="mt-0.5 block truncate text-xs text-muted-foreground">
          {item.summary}
        </span>
        <span className="mt-1 flex min-w-0 items-center gap-1.5">
          <MarketBadgeGroup badges={rowBadges(item)} limit={3} />
          <span className="shrink-0 font-mono text-[10px] text-muted-foreground">
            v{item.currentVersion.version}
          </span>
          {item.installedVersion ? (
            <span className="shrink-0 text-[10px] text-muted-foreground">
              · {t("list.installedVersion", { version: item.installedVersion })}
            </span>
          ) : null}
        </span>
      </button>
      <Button
        size="sm"
        variant={action === "update" ? "default" : "outline"}
        className="h-7 shrink-0"
        disabled={action === "none" || !artifactReady}
        title={
          action === "none"
            ? t("list.primary.noneHint")
            : !artifactReady
              ? t("detail.artifactNotReadyHint")
              : undefined
        }
        onClick={() => onPrimaryAction(item)}
      >
        {t(`list.primary.${primaryKey}`)}
      </Button>
    </div>
  )
}

function ListSkeleton() {
  return (
    <div className="space-y-1 p-3">
      {Array.from({ length: 6 }, (_, index) => (
        <Skeleton
          key={index}
          className="h-[88px] w-full rounded-md"
        />
      ))}
    </div>
  )
}

function ListError({ error, onRetry }: { error: string; onRetry: () => void }) {
  const t = useTranslations("SkillMarketV2")
  return (
    <div className="flex h-full flex-col items-center justify-center gap-3 p-6 text-center">
      <p className="text-sm font-medium">{t("list.error")}</p>
      <p className="max-w-full break-words text-xs text-muted-foreground">
        {error}
      </p>
      <Button size="sm" variant="outline" onClick={onRetry}>
        <RotateCcw className="size-3.5" aria-hidden="true" />
        {t("list.retry")}
      </Button>
    </div>
  )
}

function ListEmpty() {
  const t = useTranslations("SkillMarketV2")
  return (
    <div className="flex h-full flex-col items-center justify-center gap-2 p-6 text-center">
      <SearchX className="size-6 text-muted-foreground" aria-hidden="true" />
      <p className="text-sm font-medium">{t("list.empty")}</p>
      <p className="text-xs text-muted-foreground">{t("list.emptyHint")}</p>
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
  const [viewport, setViewport] = useState<HTMLElement | null>(null)
  const viewportRef = useRef<HTMLElement | null>(null)
  const handleViewportRef = useCallback((element: HTMLElement | null) => {
    viewportRef.current = element
    setViewport(element)
  }, [])
  const virtualizerRef = useRef<VirtualizerHandle>(null)

  const handleKeyDown = (event: React.KeyboardEvent<HTMLDivElement>) => {
    if (event.key !== "ArrowDown" && event.key !== "ArrowUp") return
    if (!props.items.length) return
    const currentIndex = props.items.findIndex(
      (item) => item.id === props.selectedId
    )
    const nextIndex =
      event.key === "ArrowDown"
        ? Math.min(props.items.length - 1, currentIndex + 1)
        : Math.max(0, currentIndex - 1)
    if (nextIndex === currentIndex) return
    event.preventDefault()
    const next = props.items[nextIndex]
    if (next) {
      props.onSelect(next)
      virtualizerRef.current?.scrollToIndex(nextIndex)
    }
  }

  if (props.loading && !props.items.length) {
    return <ListSkeleton />
  }
  if (props.error && !props.items.length) {
    return <ListError error={props.error} onRetry={props.onRetry} />
  }
  if (!props.items.length) {
    return <ListEmpty />
  }

  return (
    <div
      className="flex h-full min-h-0 flex-col"
      onKeyDown={handleKeyDown}
    >
      <div className="flex h-7 shrink-0 items-center gap-2 border-b px-3 text-[10px] text-muted-foreground">
        <span className="truncate">
          {t("list.count", { count: props.total })}
        </span>
        {props.loading ? (
          <Loader2 className="size-3 shrink-0 animate-spin" aria-hidden="true" />
        ) : null}
      </div>
      <ScrollArea className="min-h-0 flex-1" onViewportRef={handleViewportRef}>
        {viewport ? (
          <Virtualizer
            ref={virtualizerRef}
            scrollRef={viewportRef}
            data={props.items}
            itemSize={ROW_HEIGHT}
            bufferSize={600}
            shift
          >
            {(item) => (
              <MarketRow
                key={item.id}
                item={item}
                selected={props.selectedId === item.id}
                onSelect={props.onSelect}
                onPrimaryAction={props.onPrimaryAction}
              />
            )}
          </Virtualizer>
        ) : (
          <ListSkeleton />
        )}
        {props.nextCursor ? (
          <div className="p-2">
            <Button
              size="sm"
              variant="ghost"
              className="w-full"
              disabled={props.loading}
              onClick={props.onLoadMore}
            >
              {props.loading ? (
                <Loader2 className="size-3.5 animate-spin" aria-hidden="true" />
              ) : null}
              {t("list.loadMore")}
            </Button>
          </div>
        ) : null}
      </ScrollArea>
    </div>
  )
}
