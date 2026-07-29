"use client"

import {
  BadgeCheck,
  ChevronLeft,
  ChevronRight,
  Package,
  RotateCcw,
  Workflow,
} from "lucide-react"
import { useTranslations } from "next-intl"
import { Avatar, AvatarFallback, AvatarImage } from "@/components/ui/avatar"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Skeleton } from "@/components/ui/skeleton"
import type { SkillMarketItem } from "@/lib/skill-market"
import { cn } from "@/lib/utils"

type Translator = (
  key: string,
  values?: Record<string, string | number>
) => string

function categoryLabel(t: Translator, category: string): string {
  const known = [
    "office-efficiency",
    "content-creation",
    "dev-programming",
    "data-analysis",
    "design-media",
    "ai-agent",
    "knowledge-management",
    "business-ops",
    "education",
    "professional",
    "it-ops-security",
    "life-service",
  ]
  return known.includes(category) ? t(`categories.${category}`) : category
}

function MarketItemBadges({
  item,
  t,
}: {
  item: SkillMarketItem
  t: Translator
}) {
  return (
    <span className="flex min-w-0 flex-wrap items-center gap-1.5">
      <span className="max-w-full truncate text-sm font-medium">
        {item.displayName}
      </span>
      {item.publisherType === "official" ? (
        <Badge variant="outline" className="h-5 gap-1 px-1.5 text-[10px]">
          <BadgeCheck className="size-3" aria-hidden="true" />
          {t("publisher.official")}
        </Badge>
      ) : null}
      {item.visibility === "private" ? (
        <Badge variant="secondary" className="h-5 px-1.5 text-[10px]">
          {t("visibility.private")}
        </Badge>
      ) : null}
      {item.currentVersion.packageType === "expert" ? (
        <Badge variant="secondary" className="h-5 gap-1 px-1.5 text-[10px]">
          <Workflow className="size-3" aria-hidden="true" />
          {t("packageType.expert")}
        </Badge>
      ) : null}
    </span>
  )
}

function MarketItemMeta({ item, t }: { item: SkillMarketItem; t: Translator }) {
  return (
    <span className="mt-2 flex min-w-0 flex-wrap items-center gap-1.5 text-[11px] text-muted-foreground">
      <span>{categoryLabel(t, item.category)}</span>
      <span aria-hidden="true">·</span>
      <span className="font-mono">v{item.currentVersion.version}</span>
      {item.tags.slice(0, 2).map((tag) => (
        <Badge key={tag} variant="outline" className="h-5 px-1.5 text-[10px]">
          {tag}
        </Badge>
      ))}
    </span>
  )
}

function MarketItemCard({
  item,
  selected,
  onSelect,
}: {
  item: SkillMarketItem
  selected: boolean
  onSelect: () => void
}) {
  const t = useTranslations("SkillsSettings.market") as unknown as Translator
  return (
    <button
      type="button"
      onClick={onSelect}
      className={cn(
        "w-full min-w-0 rounded-lg border bg-card p-3 text-left transition-colors",
        selected
          ? "border-primary/50 bg-primary/5"
          : "hover:border-foreground/20 hover:bg-muted/20"
      )}
    >
      <div className="flex min-w-0 items-start gap-3">
        <Avatar className="size-10 rounded-md after:rounded-md">
          {item.iconUrl ? (
            <AvatarImage className="rounded-md" src={item.iconUrl} alt="" />
          ) : null}
          <AvatarFallback className="rounded-md">
            <Package className="size-4" aria-hidden="true" />
          </AvatarFallback>
        </Avatar>
        <span className="min-w-0 flex-1">
          <MarketItemBadges item={item} t={t} />
          <span className="mt-1 line-clamp-2 text-xs leading-5 text-muted-foreground">
            {item.summary}
          </span>
          <MarketItemMeta item={item} t={t} />
        </span>
      </div>
    </button>
  )
}

function ListLoading({ label }: { label: string }) {
  return (
    <div className="space-y-2" aria-label={label}>
      {Array.from({ length: 5 }, (_, index) => (
        <Skeleton key={index} className="h-24 rounded-lg" />
      ))}
    </div>
  )
}

function ListError({ error, onRetry }: { error: string; onRetry: () => void }) {
  const t = useTranslations("SkillsSettings.market")
  return (
    <div className="rounded-lg border border-destructive/30 bg-destructive/5 p-5 text-center">
      <p className="text-sm font-medium">{t("states.loadFailed")}</p>
      <p className="mt-1 break-words text-xs text-muted-foreground">{error}</p>
      <Button size="sm" variant="outline" className="mt-4" onClick={onRetry}>
        <RotateCcw className="size-3.5" />
        {t("actions.retry")}
      </Button>
    </div>
  )
}

function ListEmpty() {
  const t = useTranslations("SkillsSettings.market")
  return (
    <div className="rounded-lg border border-dashed bg-muted/10 px-4 py-10 text-center">
      <Package className="mx-auto size-6 text-muted-foreground" />
      <p className="mt-3 text-sm font-medium">{t("states.emptyTitle")}</p>
      <p className="mt-1 text-xs text-muted-foreground">
        {t("states.emptyDescription")}
      </p>
    </div>
  )
}

function PageButton({
  direction,
  disabled,
  label,
  onClick,
}: {
  direction: "previous" | "next"
  disabled: boolean
  label: string
  onClick: () => void
}) {
  const Icon = direction === "previous" ? ChevronLeft : ChevronRight
  return (
    <Button
      size="icon-sm"
      variant="outline"
      disabled={disabled}
      aria-label={label}
      title={label}
      onClick={onClick}
    >
      <Icon className="size-3.5" aria-hidden="true" />
    </Button>
  )
}

function ListPagination({
  page,
  pageSize,
  total,
  onPageChange,
}: {
  page: number
  pageSize: number
  total: number
  onPageChange: (page: number) => void
}) {
  const t = useTranslations("SkillsSettings.market")
  const safePageSize = Math.max(1, pageSize)
  const totalPages = Math.max(1, Math.ceil(total / safePageSize))
  if (totalPages <= 1) return null
  const currentPage = Math.min(Math.max(1, page), totalPages)
  const start = (currentPage - 1) * safePageSize + 1
  const end = Math.min(currentPage * safePageSize, total)
  return (
    <nav
      className="mt-4 flex flex-wrap items-center justify-between gap-3 border-t pt-3"
      aria-label={t("pagination.navigation")}
    >
      <span className="text-xs text-muted-foreground">
        {t("pagination.summary", { start, end, total })}
      </span>
      <div className="flex items-center gap-2">
        <PageButton
          direction="previous"
          disabled={currentPage <= 1}
          label={t("pagination.previous")}
          onClick={() => onPageChange(currentPage - 1)}
        />
        <span className="min-w-16 text-center text-xs text-muted-foreground">
          {t("pagination.page", { page: currentPage, pages: totalPages })}
        </span>
        <PageButton
          direction="next"
          disabled={currentPage >= totalPages}
          label={t("pagination.next")}
          onClick={() => onPageChange(currentPage + 1)}
        />
      </div>
    </nav>
  )
}

export function SkillMarketList({
  items,
  selectedId,
  loading,
  error,
  page,
  pageSize,
  total,
  onSelect,
  onRetry,
  onPageChange,
}: {
  items: SkillMarketItem[]
  selectedId: string | null
  loading: boolean
  error: string | null
  page: number
  pageSize: number
  total: number
  onSelect: (item: SkillMarketItem) => void
  onRetry: () => void
  onPageChange: (page: number) => void
}) {
  const t = useTranslations("SkillsSettings.market")
  if (loading) return <ListLoading label={t("states.loading")} />
  if (error) return <ListError error={error} onRetry={onRetry} />
  if (!items.length) return <ListEmpty />
  return (
    <div>
      <div className="space-y-2">
        {items.map((item) => (
          <MarketItemCard
            key={item.id}
            item={item}
            selected={selectedId === item.id}
            onSelect={() => onSelect(item)}
          />
        ))}
      </div>
      <ListPagination
        page={page}
        pageSize={pageSize}
        total={total}
        onPageChange={onPageChange}
      />
    </div>
  )
}
