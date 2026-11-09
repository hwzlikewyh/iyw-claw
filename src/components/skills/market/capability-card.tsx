"use client"

import { ArrowRight, Check } from "lucide-react"
import { useTranslations } from "next-intl"
import { Button } from "@/components/ui/button"
import { MarketItemIcon } from "./market-item-icon"

export const CAPABILITY_GRID_CLASS =
  "grid grid-cols-[repeat(auto-fill,minmax(min(100%,260px),1fr))] gap-3"

export interface CapabilityCardProps {
  kind: "skill" | "connector" | "plugin"
  name: string
  publisher: string
  summary: string
  category: string
  source: string
  version: string | null
  icon?: string | null
  installed: boolean
  status?: string
  onOpen: () => void
}

export function CapabilityCard(props: CapabilityCardProps) {
  const t = useTranslations("CapabilityMarket.catalog")
  return (
    <article className="flex min-h-[176px] min-w-0 flex-col gap-2 rounded-[10px] border bg-background p-3.5 hover:border-foreground/25 focus-within:outline-2 focus-within:outline-offset-2 focus-within:outline-ring">
      <header className="flex min-w-0 items-center gap-2">
        <MarketItemIcon
          name={props.name}
          src={props.icon}
          kind={props.kind}
          className="size-6"
        />
        <div className="min-w-0 flex-1">
          <button
            type="button"
            data-market-card-title
            className="block min-h-5 max-w-full truncate text-left text-sm font-semibold"
            title={props.name}
            onClick={props.onOpen}
          >
            {props.name}
          </button>
          <p className="mt-0.5 truncate text-xs text-muted-foreground">
            {props.publisher}
          </p>
        </div>
        <span className="shrink-0 text-xs text-muted-foreground">
          {t(props.kind)}
        </span>
      </header>
      <p className="line-clamp-2 min-h-10 break-words text-[13px] leading-5 text-muted-foreground">
        {props.summary || t("noDescription")}
      </p>
      <p className="flex min-w-0 items-center gap-1.5 text-xs text-muted-foreground">
        <span className="truncate">{props.category || t("uncategorized")}</span>
        <span aria-hidden className="shrink-0">
          ·
        </span>
        <span className="truncate">{props.source}</span>
      </p>
      {props.status && (
        <p className="text-xs text-muted-foreground">{props.status}</p>
      )}
      <CardFooter {...props} />
    </article>
  )
}

function CardFooter(props: CapabilityCardProps) {
  const t = useTranslations("CapabilityMarket.catalog")
  return (
    <footer className="mt-auto flex items-center justify-between gap-2 border-t pt-2 text-xs">
      <span className="flex min-w-0 items-center gap-1 truncate text-muted-foreground">
        {props.installed && <Check className="size-3.5 shrink-0" />}
        <span className="truncate">
          {t(props.installed ? "installedVersion" : "version", {
            version: props.version || "—",
          })}
        </span>
      </span>
      <Button
        variant="outline"
        size="sm"
        className="min-h-8 shrink-0 rounded-md px-2 text-xs max-sm:min-h-11"
        onClick={props.onOpen}
      >
        {t(props.installed ? "manage" : "details")}
        <ArrowRight className="size-3.5" />
      </Button>
    </footer>
  )
}
