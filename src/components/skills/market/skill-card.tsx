"use client"

import { ArrowUpRight } from "lucide-react"
import { useTranslations } from "next-intl"
import { Button } from "@/components/ui/button"
import { MarketBadgeGroup } from "@/components/skills/market/badges"
import { MarketItemIcon } from "@/components/skills/market/market-item-icon"
import {
  audienceBadgeInfo,
  compatibilityBadgeInfo,
  installStateBadgeInfo,
  primaryInstallAction,
  type MarketBadgeInfo,
  type SkillMarketTranslator,
  type SkillMarketV2Item,
} from "@/lib/skill-market"
import { cn } from "@/lib/utils"

const MARKET_CARD_BASE_CLASS =
  "group flex h-52 min-w-0 flex-col overflow-hidden rounded-lg border bg-background p-4 transition-colors"

export function marketCardClass(selected: boolean): string {
  return cn(
    MARKET_CARD_BASE_CLASS,
    selected
      ? "border-primary/40 bg-primary/[0.03]"
      : "hover:border-foreground/25 hover:bg-muted/20"
  )
}

function itemBadges(item: SkillMarketV2Item): MarketBadgeInfo[] {
  return [
    audienceBadgeInfo(item.audience),
    ...(item.packageType === "plugin"
      ? [
          {
            key: "package.plugin",
            tone: "primary" as const,
            icon: "package" as const,
          },
        ]
      : []),
    ...(item.compatibility === "incompatible"
      ? [compatibilityBadgeInfo(item.compatibility)]
      : []),
    ...(item.installState !== "not_installed"
      ? [installStateBadgeInfo(item.installState)]
      : []),
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
        <MarketItemIcon name={item.displayName} src={item.iconUrl} />
        <span className="min-w-0 flex-1">
          <span
            className="block truncate text-sm font-semibold"
            title={item.displayName}
          >
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
        className="mt-3 h-5 shrink-0 overflow-hidden"
      />
      <span className="mt-2 line-clamp-2 h-10 shrink-0 overflow-hidden break-words text-xs leading-5 text-muted-foreground [overflow-wrap:anywhere]">
        {item.summary || t("inventory.noDescription")}
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
    <div className="mt-2.5 flex min-w-0 shrink-0 items-center justify-between gap-2 border-t pt-2.5">
      <span className="truncate font-mono text-[10px] text-muted-foreground">
        v{item.currentVersion.version}
      </span>
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
