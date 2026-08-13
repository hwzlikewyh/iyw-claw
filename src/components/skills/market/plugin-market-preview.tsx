"use client"

import {
  ArrowRight,
  Blocks,
  Braces,
  PackageCheck,
  PlugZap,
  Sparkles,
  Workflow,
} from "lucide-react"
import { useTranslations } from "next-intl"
import { ScrollArea } from "@/components/ui/scroll-area"

const BUNDLES = [
  { id: "release", icon: PackageCheck, tone: "bg-blue-50 dark:bg-blue-950/35" },
  {
    id: "knowledge",
    icon: Braces,
    tone: "bg-emerald-50 dark:bg-emerald-950/35",
  },
  { id: "quality", icon: Workflow, tone: "bg-amber-50 dark:bg-amber-950/35" },
] as const

const BUNDLE_PARTS = ["skills", "connectors", "bindings"] as const

export function PluginMarketPreview() {
  return (
    <ScrollArea className="h-full">
      <div className="px-4 py-5 sm:px-6 lg:py-6">
        <PluginHero />
        <PluginCatalog />
      </div>
    </ScrollArea>
  )
}

function PluginHero() {
  const t = useTranslations("CapabilityMarket.plugins")
  return (
    <section className="-mx-4 -mt-5 grid gap-6 border-b bg-muted/[0.16] px-4 py-6 sm:-mx-6 sm:px-6 lg:-mt-6 lg:grid-cols-[minmax(0,1fr)_26rem] lg:items-center">
      <div className="max-w-2xl">
        <span className="inline-flex items-center gap-1.5 text-[10px] font-semibold uppercase text-primary">
          <Blocks className="size-3" aria-hidden="true" />
          {t("eyebrow")}
        </span>
        <h2 className="mt-3 text-lg font-semibold">{t("title")}</h2>
        <p className="mt-2 max-w-xl text-sm leading-6 text-muted-foreground">
          {t("description")}
        </p>
      </div>
      <div className="flex min-w-0 items-center justify-center gap-2 sm:gap-3">
        <PackagePart icon={Sparkles} label={t("parts.skills")} />
        <ArrowRight className="size-4 shrink-0 text-muted-foreground" />
        <PackagePart icon={PlugZap} label={t("parts.connectors")} />
        <ArrowRight className="size-4 shrink-0 text-muted-foreground" />
        <PackagePart icon={Braces} label={t("parts.bindings")} />
      </div>
    </section>
  )
}

function PluginCatalog() {
  const t = useTranslations("CapabilityMarket.plugins")
  return (
    <section className="pt-7">
      <div className="mb-4 flex items-end justify-between gap-4">
        <div>
          <h3 className="text-sm font-semibold">{t("catalogTitle")}</h3>
          <p className="mt-1 text-[11px] text-muted-foreground">
            {t("catalogSubtitle")}
          </p>
        </div>
        <span className="rounded border bg-muted/35 px-2 py-1 text-[10px] text-muted-foreground">
          {t("comingSoon")}
        </span>
      </div>
      <div className="grid gap-3 md:grid-cols-2 xl:grid-cols-3">
        {BUNDLES.map((bundle) => (
          <BundleCard key={bundle.id} bundle={bundle} />
        ))}
      </div>
    </section>
  )
}

function BundleCard({ bundle }: { bundle: (typeof BUNDLES)[number] }) {
  const t = useTranslations("CapabilityMarket.plugins")
  const Icon = bundle.icon
  return (
    <article className="group flex min-h-44 flex-col rounded-lg border bg-background p-4 transition-[border-color,box-shadow,transform] hover:-translate-y-0.5 hover:border-foreground/20 hover:shadow-md">
      <div className="flex items-start justify-between gap-3">
        <span
          className={`flex size-9 items-center justify-center rounded-md ${bundle.tone}`}
        >
          <Icon className="size-4" aria-hidden="true" />
        </span>
        <span className="text-[10px] font-medium text-muted-foreground">
          {t("comingSoon")}
        </span>
      </div>
      <h4 className="mt-4 text-sm font-semibold">
        {t(`bundles.${bundle.id}.title`)}
      </h4>
      <p className="mt-1 text-xs leading-5 text-muted-foreground">
        {t(`bundles.${bundle.id}.description`)}
      </p>
      <div className="mt-auto flex flex-wrap gap-1.5 pt-4">
        {BUNDLE_PARTS.map((part) => (
          <span
            key={part}
            className="rounded border bg-muted/25 px-2 py-1 text-[10px] text-muted-foreground"
          >
            {t(`parts.${part}`)}
          </span>
        ))}
      </div>
    </article>
  )
}

function PackagePart({
  icon: Icon,
  label,
}: {
  icon: typeof Sparkles
  label: string
}) {
  return (
    <div className="flex min-w-0 flex-1 flex-col items-center gap-2 rounded-md border bg-background px-2 py-3 text-center">
      <Icon className="size-4" aria-hidden="true" />
      <span className="truncate text-[11px] font-medium">{label}</span>
    </div>
  )
}
