"use client"

import { ArrowRight, Check } from "lucide-react"
import { useTranslations } from "next-intl"
import { Button } from "@/components/ui/button"
import { MarketItemIcon } from "./market-item-icon"

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
    <article className="flex min-h-[208px] min-w-0 flex-col gap-3 rounded-[10px] border bg-background p-[17px] hover:border-foreground/25 focus-within:outline-2 focus-within:outline-offset-2 focus-within:outline-ring">
      <header className="flex min-w-0 items-center gap-[11px]">
        <MarketItemIcon
          name={props.name}
          src={props.icon}
          kind={props.kind}
          className="size-7"
        />
        <div className="min-w-0 flex-1">
          <button
            type="button"
            data-market-card-title
            className="block min-h-6 max-w-full truncate text-left text-base font-semibold"
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
      <p className="line-clamp-2 min-h-12 break-words text-sm leading-[1.7] text-muted-foreground">
        {props.summary || t("noDescription")}
      </p>
      <p className="flex flex-wrap items-center gap-2 text-[13px] text-muted-foreground">
        <span>{props.category || t("uncategorized")}</span>
        <span aria-hidden>·</span>
        <span>{props.source}</span>
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
    <footer className="mt-auto flex items-center justify-between gap-2 border-t pt-2.5 text-[13px]">
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
        className="min-h-8 shrink-0 rounded-md px-2.5 text-[13px] max-sm:min-h-11"
        onClick={props.onOpen}
      >
        {t(props.installed ? "manage" : "details")}
        <ArrowRight className="size-3.5" />
      </Button>
    </footer>
  )
}
