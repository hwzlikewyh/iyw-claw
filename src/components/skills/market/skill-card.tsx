"use client"
import { useTranslations } from "next-intl"
import type { SkillMarketV2Item } from "@/lib/skill-market"
import { CapabilityCard } from "./capability-card"

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
  const source =
    item.audience === "organization"
      ? t("organization")
      : t(item.publisher === "official" ? "official" : "community")
  return (
    <CapabilityCard
      kind={item.packageType === "plugin" ? "plugin" : "skill"}
      name={item.displayName}
      publisher={item.organizationName || source}
      summary={item.summary}
      category={item.category}
      source={source}
      version={item.installedVersion || item.currentVersion.version}
      icon={item.iconUrl}
      installed={item.installedVersion != null}
      onOpen={() => onSelect(item)}
    />
  )
}
