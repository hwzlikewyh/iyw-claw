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
    <CapabilityCard
      kind="plugin"
      name={item.displayName}
      publisher={item.organizationName || source}
      summary={item.summary}
      category={item.category}
      source={source}
      version={item.installedVersion || item.currentVersion.version}
      icon={item.iconUrl}
      installed={item.installedVersion != null}
      status={status ? t(`status.${status}`) : undefined}
      onOpen={onOpen}
    />
  )
}
