"use client"

import { useTranslations } from "next-intl"
import { McpSettings } from "@/components/settings/mcp-settings"

export function ConnectorMarketView() {
  const t = useTranslations("CapabilityMarket.connectors")

  return (
    <div className="flex h-full min-h-0 flex-col bg-background">
      <section className="shrink-0 border-b bg-muted/[0.12] px-4 py-5 sm:px-6">
        <div className="max-w-3xl">
          <div className="max-w-2xl">
            <p className="text-[10px] font-semibold uppercase text-muted-foreground">
              {t("eyebrow")}
            </p>
            <h2 className="mt-1.5 text-lg font-semibold">{t("title")}</h2>
            <p className="mt-1.5 text-xs leading-5 text-muted-foreground">
              {t("description")}
            </p>
          </div>
        </div>
      </section>
      <div className="min-h-0 flex-1">
        <McpSettings embedded />
      </div>
    </div>
  )
}
