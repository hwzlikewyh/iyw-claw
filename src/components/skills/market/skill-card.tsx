"use client"

import { Boxes, CalendarClock, HardDrive, Package } from "lucide-react"
import { useLocale, useTranslations } from "next-intl"
import { Avatar, AvatarFallback, AvatarImage } from "@/components/ui/avatar"
import { Button } from "@/components/ui/button"
import { MarketBadgeGroup } from "@/components/skills/market/badges"
import {
  audienceBadgeInfo,
  compatibilityBadgeInfo,
  formatSkillBytes,
  installStateBadgeInfo,
  primaryInstallAction,
  type MarketBadgeInfo,
  type SkillMarketTranslator,
  type SkillMarketV2Item,
} from "@/lib/skill-market"
import { cn } from "@/lib/utils"

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
    installStateBadgeInfo(item.installState),
    audienceBadgeInfo(item.audience),
    ...(item.compatibility !== "compatible"
      ? [compatibilityBadgeInfo(item.compatibility)]
      : []),
  ]
}

function formatDate(locale: string, value: string): string {
  const date = new Date(value)
  if (Number.isNaN(date.getTime())) return "-"
  return new Intl.DateTimeFormat(locale, {
    month: "short",
    day: "numeric",
  }).format(date)
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
  const locale = useLocale()
  const action = primaryInstallAction(item.installState, item.compatibility)
  const artifactReady = item.currentVersion.status === "ready"
  const primaryKey = !artifactReady
    ? item.currentVersion.status === "artifact_pending"
      ? "waitingArtifact"
      : "buildFailed"
    : action
  return (
    <article
      className={cn(
        "flex h-[17rem] min-w-0 flex-col border bg-background p-3.5 transition-colors",
        selected
          ? "border-primary/60 shadow-[inset_0_3px_0_hsl(var(--primary))]"
          : "hover:border-foreground/25"
      )}
    >
      <SkillCardSummary item={item} selected={selected} onSelect={onSelect} />
      <SkillCardMeta item={item} locale={locale} />
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
      className="min-w-0 text-left outline-none focus-visible:ring-2 focus-visible:ring-ring/50"
      onClick={() => onSelect(item)}
      aria-current={selected ? "true" : undefined}
      aria-label={t("a11y.openDetail", { name: item.displayName })}
    >
      <span className="flex min-w-0 items-start gap-3">
        <Avatar className="size-9 shrink-0 rounded-md">
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
          <span className="mt-0.5 block truncate font-mono text-[10px] text-muted-foreground">
            {item.slug}
          </span>
        </span>
      </span>
      <MarketBadgeGroup
        badges={itemBadges(item)}
        limit={4}
        className="mt-3 h-5 overflow-hidden"
      />
      <span className="mt-3 line-clamp-3 block min-h-[3.75rem] text-xs leading-5 text-muted-foreground">
        {item.summary}
      </span>
    </button>
  )
}

function SkillCardMeta({
  item,
  locale,
}: {
  item: SkillMarketV2Item
  locale: string
}) {
  const t = useTranslations("SkillMarketV2")
  const components = item.currentVersion.plugin?.components ?? []
  const skillCount = components.filter((value) => value.type === "skill").length
  const connectorCount = components.filter(
    (value) => value.type === "connector"
  ).length
  const dependencySummary =
    item.packageType === "plugin"
      ? t("list.pluginComponents", {
          skills: skillCount,
          connectors: connectorCount,
        })
      : t("list.dependencyCount", {
          count: item.currentVersion.dependencies.length,
        })
  return (
    <div className="mt-auto grid grid-cols-2 gap-x-2 gap-y-1.5 border-t pt-2.5 text-[10px] text-muted-foreground">
      <CardMeta icon={Package} value={`v${item.currentVersion.version}`} />
      <CardMeta
        icon={HardDrive}
        value={formatSkillBytes(item.currentVersion.artifactSize)}
      />
      <CardMeta icon={Boxes} value={dependencySummary} />
      <CardMeta
        icon={CalendarClock}
        value={formatDate(locale, item.updatedAt)}
      />
    </div>
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
    <div className="mt-2.5 flex min-w-0 items-center gap-2">
      <div className="flex min-w-0 flex-1 gap-1 overflow-hidden">
        {item.tags.slice(0, 2).map((tag) => (
          <span
            key={tag}
            className="max-w-24 truncate border bg-muted/30 px-1.5 py-0.5 text-[9px] text-muted-foreground"
          >
            {tag}
          </span>
        ))}
      </div>
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

function CardMeta({
  icon: Icon,
  value,
}: {
  icon: typeof Package
  value: string
}) {
  return (
    <span className="flex min-w-0 items-center gap-1.5">
      <Icon className="size-3 shrink-0" aria-hidden="true" />
      <span className="truncate">{value}</span>
    </span>
  )
}
