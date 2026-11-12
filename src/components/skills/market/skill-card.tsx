"use client"
import { useTranslations } from "next-intl"
import type { SkillMarketV2Item } from "@/lib/skill-market"
import { CapabilityCard } from "./capability-card"
import { capabilityStatus } from "./capability-status"

export function marketCardClass(selected: boolean): string {
  return (
    "group flex min-h-[208px] min-w-0 flex-col overflow-hidden rounded-[10px] border bg-background p-[17px] hover:border-foreground/25" +
    (selected ? " ring-1 ring-border" : "")
  )
}

export function SkillCard({
  item,
  onSelect,
}: {
  item: SkillMarketV2Item
  selected: boolean
  onSelect: (item: SkillMarketV2Item) => void
  onPrimaryAction: (item: SkillMarketV2Item) => void
}) {
  const t = useTranslations("CapabilityMarket.catalog")
  const status = capabilityStatus(item)
  const source =
    item.audience === "organization"
      ? t("organization")
      : t(item.publisher === "official" ? "official" : "community")
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
          <span className="mt-0.5 block truncate text-[11px] text-muted-foreground">
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
      <span className="truncate font-mono text-[11px] text-muted-foreground">
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
