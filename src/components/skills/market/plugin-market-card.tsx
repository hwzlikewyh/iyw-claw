"use client"
import { useTranslations } from "next-intl"
import type { SkillMarketV2Item } from "@/lib/skill-market"
import { CapabilityCard } from "./capability-card"
import { capabilityStatus } from "./capability-status"

export function PluginMarketCard({
  item,
  onOpen,
}: {
  item: SkillMarketV2Item
  onOpen: () => void
}) {
  const t = useTranslations("CapabilityMarket.catalog")
  const status = capabilityStatus(item)
  const source =
    item.audience === "organization"
      ? t("organization")
      : t(item.publisher === "official" ? "official" : "community")
  return (
    <button
      type="button"
      className={cn(
        marketCardClass(false),
        "text-left outline-none focus-visible:ring-2 focus-visible:ring-ring/50"
      )}
      aria-label={t("openDetail", { name: item.displayName })}
      onClick={onOpen}
    >
      <PluginCardHeading item={item} />
      <MarketBadgeGroup
        badges={badges}
        className="mt-3 h-5 shrink-0 overflow-hidden"
      />
      <span className="mt-2 line-clamp-2 h-10 shrink-0 break-words text-xs leading-5 text-muted-foreground">
        {item.summary || market("inventory.noDescription")}
      </span>
      <ComponentCounts item={item} />
      <span className="mt-auto flex w-full min-w-0 items-center justify-between gap-2 border-t pt-2 text-[11px] text-muted-foreground">
        <span className="truncate font-mono">
          v{item.currentVersion.version}
        </span>
        <span className="shrink-0">
          {t(`publisherValues.${item.publisher}`)}
        </span>
      </span>
    </button>
  )
}

function PluginCardHeading({ item }: { item: SkillMarketV2Item }) {
  return (
    <span className="flex w-full min-w-0 items-start gap-3">
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
        className="size-3.5 shrink-0 text-muted-foreground"
        aria-hidden="true"
      />
    </span>
  )
}
