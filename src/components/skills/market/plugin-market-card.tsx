"use client"

import {
  AppWindow,
  ArrowUpRight,
  Boxes,
  PlugZap,
  Sparkles,
  Wrench,
} from "lucide-react"
import { useTranslations } from "next-intl"
import { MarketBadgeGroup } from "./badges"
import { MarketItemIcon } from "./market-item-icon"
import { marketCardClass } from "./skill-card"
import {
  artifactStatusBadgeInfo,
  audienceBadgeInfo,
  installStateBadgeInfo,
  type SkillMarketV2Item,
} from "@/lib/skill-market"
import { cn } from "@/lib/utils"

const COMPONENT_ICONS = {
  skill: Sparkles,
  connector: PlugZap,
  runtime: Boxes,
  capability: Wrench,
  app: AppWindow,
} as const

function ComponentCounts({ item }: { item: SkillMarketV2Item }) {
  const t = useTranslations("CapabilityMarket.plugins")
  const components = item.currentVersion.plugin?.components ?? []
  return (
    <span className="mt-2 flex h-5 items-center gap-3 text-xs text-muted-foreground">
      {(Object.keys(COMPONENT_ICONS) as (keyof typeof COMPONENT_ICONS)[]).map(
        (type) => {
          const count = components.filter(
            (component) => component.type === type
          ).length
          if (!count) return null
          const Icon = COMPONENT_ICONS[type]
          return (
            <span
              key={type}
              className="inline-flex items-center gap-1"
              title={`${t(`componentType.${type}`)}: ${count}`}
            >
              <Icon className="size-3.5" aria-hidden="true" />
              <span className="sr-only">{t(`componentType.${type}`)}</span>
              {count}
            </span>
          )
        }
      )}
    </span>
  )
}

export function PluginMarketCard({
  item,
  onOpen,
}: {
  item: SkillMarketV2Item
  onOpen: () => void
}) {
  const t = useTranslations("CapabilityMarket.plugins")
  const market = useTranslations("SkillMarketV2")
  const badges = [
    audienceBadgeInfo(item.audience),
    installStateBadgeInfo(item.installState),
  ]
  if (item.currentVersion.status !== "ready")
    badges.push(artifactStatusBadgeInfo(item.currentVersion.status))
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
      <span className="mt-auto flex w-full min-w-0 items-center justify-between gap-2 border-t pt-2 text-[10px] text-muted-foreground">
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
        <span className="mt-0.5 block truncate text-[10px] text-muted-foreground">
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
