"use client"

import { ArrowUpRight, Package } from "lucide-react"
import { useTranslations } from "next-intl"
import { Avatar, AvatarFallback, AvatarImage } from "@/components/ui/avatar"
import { Button } from "@/components/ui/button"
import { MarketBadgeGroup } from "@/components/skills/market/badges"
import {
  audienceBadgeInfo,
  compatibilityBadgeInfo,
  primaryInstallAction,
  type MarketBadgeInfo,
  type SkillMarketTranslator,
  type SkillMarketV2Item,
} from "@/lib/skill-market"
import { cn } from "@/lib/utils"

const MARKET_CARD_BASE_CLASS =
  "group flex h-[12.5rem] min-w-0 flex-col overflow-hidden rounded-lg border bg-background p-3.5 transition-[border-color,box-shadow,transform]"

export function marketCardClass(selected: boolean): string {
  return cn(
    MARKET_CARD_BASE_CLASS,
    selected
      ? "border-foreground/35 shadow-[inset_3px_0_0_hsl(var(--foreground))]"
      : "hover:-translate-y-0.5 hover:border-foreground/20 hover:shadow-[0_8px_22px_rgba(15,23,42,0.055)]"
  )
}

function itemBadges(item: SkillMarketV2Item): MarketBadgeInfo[] {
  return [
    ...(item.packageType === "plugin"
      ? [
          {
            key: "package.plugin",
            tone: "primary" as const,
            icon: "package" as const,
          },
        ]
      : []),
    ...(item.compatibility !== "compatible"
      ? [compatibilityBadgeInfo(item.compatibility)]
      : []),
    audienceBadgeInfo(item.audience),
  ]
}

export function SkillCard({
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
  const action = primaryInstallAction(item.installState, item.compatibility)
  const artifactReady = item.currentVersion.status === "ready"
  const primaryKey = !artifactReady
    ? item.currentVersion.status === "artifact_pending"
      ? "waitingArtifact"
      : "buildFailed"
    : action
  return (
    <article className={marketCardClass(selected)}>
      <SkillCardSummary item={item} selected={selected} onSelect={onSelect} />
      <SkillCardFooter
        item={item}
        action={action}
        primaryKey={primaryKey}
        disabled={action === "none" || !artifactReady}
        onPrimaryAction={onPrimaryAction}
      />
    </article>
  )
}

function SkillCardSummary({
  item,
  selected,
  onSelect,
}: {
  item: SkillMarketV2Item
  selected: boolean
  onSelect: (item: SkillMarketV2Item) => void
}) {
  const t = useTranslations("SkillMarketV2")
  return (
    <button
      type="button"
      className="flex min-h-0 min-w-0 flex-1 flex-col overflow-hidden text-left outline-none focus-visible:ring-2 focus-visible:ring-ring/50"
      onClick={() => onSelect(item)}
      aria-current={selected ? "true" : undefined}
      aria-label={t("a11y.openDetail", { name: item.displayName })}
    >
      <span className="flex min-w-0 items-start gap-3">
        <Avatar className="size-10 shrink-0 rounded-md border bg-muted/35">
          {item.iconUrl ? (
            <AvatarImage className="rounded-md" src={item.iconUrl} alt="" />
          ) : null}
          <AvatarFallback className="rounded-md">
            <Package className="size-4" />
          </AvatarFallback>
        </Avatar>
        <span className="min-w-0 flex-1">
          <span className="block truncate text-sm font-semibold">
            {item.displayName}
          </span>
          <span className="mt-0.5 block truncate text-[10px] text-muted-foreground">
            {item.organizationName ?? item.slug}
          </span>
        </span>
        <ArrowUpRight
          className="size-3.5 shrink-0 text-muted-foreground/70 transition-transform group-hover:translate-x-0.5 group-hover:-translate-y-0.5"
          aria-hidden="true"
        />
      </span>
      <MarketBadgeGroup
        badges={itemBadges(item)}
        limit={3}
        className="mt-2.5 h-5 shrink-0 overflow-hidden"
      />
      <span className="mt-2 line-clamp-2 h-10 shrink-0 overflow-hidden break-words text-xs leading-5 text-muted-foreground [overflow-wrap:anywhere]">
        {item.summary}
      </span>
    </button>
  )
}

function SkillCardFooter({
  item,
  action,
  primaryKey,
  disabled,
  onPrimaryAction,
}: {
  item: SkillMarketV2Item
  action: ReturnType<typeof primaryInstallAction>
  primaryKey: string
  disabled: boolean
  onPrimaryAction: (item: SkillMarketV2Item) => void
}) {
  const t = useTranslations("SkillMarketV2") as unknown as SkillMarketTranslator
  return (
    <div className="mt-2.5 flex min-w-0 shrink-0 items-center justify-end gap-2 border-t pt-2.5">
      <Button
        size="xs"
        variant={action === "update" ? "default" : "outline"}
        className="shrink-0"
        disabled={disabled}
        onClick={() => onPrimaryAction(item)}
      >
        {t(`list.primary.${primaryKey}`)}
      </Button>
    </div>
  )
}
