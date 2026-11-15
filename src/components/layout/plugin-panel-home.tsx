"use client"

import { ChevronRight, RefreshCw } from "lucide-react"
import { useTranslations } from "next-intl"
import { Button } from "@/components/ui/button"
import type { usePluginPanels } from "./use-plugin-panels"
import { usePluginMarketNavigation } from "./use-plugin-market-navigation"
import { InstalledPluginTools } from "./installed-plugin-tools"

type Panels = ReturnType<typeof usePluginPanels>

export function PluginPanelHome({ panels }: { panels: Panels }) {
  const t = useTranslations("PluginPanels")
  return (
    <section className="min-w-0 shrink-0 px-3 pb-4 pt-2">
      <PluginListHeading refresh={panels.refresh} />
      {panels.error && (
        <p role="alert" className="break-words py-2 text-sm text-destructive">
          {panels.error === "PluginPanels.limit" ? t("limit") : panels.error}
        </p>
      )}
      <InstalledPluginTools panels={panels} />
    </section>
  )
}

function PluginListHeading({ refresh }: { refresh: () => void }) {
  const t = useTranslations("PluginPanels")
  const route = usePluginMarketNavigation()
  return (
    <div className="mb-2 flex flex-wrap items-center gap-2 px-2">
      <h2 className="w-full min-w-0 text-sm font-semibold @min-[320px]:w-auto @min-[320px]:flex-1">
        {t("apps")}
      </h2>
      <Button variant="link" size="xs" onClick={route.openPluginMarket}>
        {t("all")}
        <ChevronRight className="size-3" />
      </Button>
      <Button
        variant="ghost"
        size="icon-xs"
        aria-label={t("refresh")}
        onClick={refresh}
      >
        <RefreshCw className="size-3.5" />
      </Button>
    </div>
  )
}
