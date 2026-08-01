"use client"

import {
  AlertTriangle,
  ArrowUp,
  Ban,
  Building2,
  Check,
  Clock,
  Globe,
  Lock,
  Package,
  ShieldCheck,
  Wrench,
  type LucideIcon,
} from "lucide-react"
import { useTranslations } from "next-intl"
import { Badge } from "@/components/ui/badge"
import type {
  MarketBadgeIcon,
  MarketBadgeInfo,
  MarketBadgeTone,
  SkillMarketTranslator,
} from "@/lib/skill-market"
import { cn } from "@/lib/utils"

const ICONS: Record<MarketBadgeIcon, LucideIcon> = {
  globe: Globe,
  building: Building2,
  lock: Lock,
  shield: ShieldCheck,
  check: Check,
  arrowUp: ArrowUp,
  clock: Clock,
  ban: Ban,
  alert: AlertTriangle,
  package: Package,
  wrench: Wrench,
}

const TONES: Record<MarketBadgeTone, string> = {
  default: "",
  primary: "border-primary/30 bg-primary/10 text-primary",
  success:
    "border-emerald-500/30 bg-emerald-500/10 text-emerald-700 dark:text-emerald-400",
  warning:
    "border-amber-500/30 bg-amber-500/10 text-amber-700 dark:text-amber-400",
  danger: "border-destructive/30 bg-destructive/10 text-destructive",
  muted: "border-border bg-muted/40 text-muted-foreground",
}

export function MarketBadge({
  info,
  className,
}: {
  info: MarketBadgeInfo
  className?: string
}) {
  const t = useTranslations("SkillMarketV2") as unknown as SkillMarketTranslator
  const Icon = info.icon ? ICONS[info.icon] : null
  return (
    <Badge
      variant="outline"
      className={cn("h-5 px-1.5 text-[10px]", TONES[info.tone], className)}
      title={t(info.key)}
    >
      {Icon ? <Icon aria-hidden="true" /> : null}
      <span className="truncate">{t(info.key)}</span>
    </Badge>
  )
}

export function MarketBadgeGroup({
  badges,
  limit = 3,
  className,
}: {
  badges: MarketBadgeInfo[]
  limit?: number
  className?: string
}) {
  const visible = badges.slice(0, limit)
  return (
    <span
      className={cn(
        "flex min-w-0 flex-wrap items-center gap-1.5",
        className
      )}
    >
      {visible.map((info) => (
        <MarketBadge key={`${info.key}-${info.tone}`} info={info} />
      ))}
    </span>
  )
}
