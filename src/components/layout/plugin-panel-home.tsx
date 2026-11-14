"use client"

import { AppWindow, ChevronRight, RefreshCw } from "lucide-react"
import { useTranslations } from "next-intl"
import { Button } from "@/components/ui/button"
import type { PluginPanelChoice } from "@/lib/plugin-panel"
import type { usePluginPanels } from "./use-plugin-panels"
import { usePluginMarketNavigation } from "./use-plugin-market-navigation"
import { InstalledPluginTools } from "./installed-plugin-tools"

type Panels = ReturnType<typeof usePluginPanels>

export function PluginPanelHome({ panels }: { panels: Panels }) {
  const t = useTranslations("PluginPanels")
  const open = (app: PluginPanelChoice) => void panels.open(app)
  return (
    <section className="px-3 pb-4 pt-2">
      <PluginListHeading refresh={panels.refresh} />
      {panels.loading && (
        <p role="status" className="py-2 text-sm text-muted-foreground">
          {t("loading")}
        </p>
      )}
      {panels.error && (
        <p role="alert" className="break-words py-2 text-sm text-destructive">
          {panels.error === "PluginPanels.limit" ? t("limit") : panels.error}
        </p>
      )}
      <ul className="grid grid-cols-1 gap-x-4 gap-y-1 @min-[480px]:grid-cols-2">
        {panels.choices.map((app) => (
          <InstalledApp
            key={`${app.pluginSlug}:${app.appKey}`}
            app={app}
            busy={panels.busy}
            open={() => open(app)}
          />
        ))}
      </ul>
      <InstalledPluginTools panels={panels} />
    </section>
  )
}

function PluginListHeading({ refresh }: { refresh: () => void }) {
  const t = useTranslations("PluginPanels")
  const route = usePluginMarketNavigation()
  return (
    <div className="mb-3 flex items-center gap-2">
      <h2 className="flex-1 text-sm font-semibold">{t("apps")}</h2>
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

function InstalledApp({
  app,
  busy,
  open,
}: {
  app: PluginPanelChoice
  busy: boolean
  open: () => void
}) {
  const t = useTranslations("PluginPanels")
  return (
    <li className="flex min-h-16 min-w-0 items-center gap-3">
      <AppWindow className="size-5 shrink-0 text-muted-foreground" />
      <div className="min-w-0 flex-1">
        <p className="truncate text-sm font-medium" title={app.label}>
          {app.label}
        </p>
        <p className="truncate text-xs text-muted-foreground">
          {t("installedVersion", { version: app.version })}
        </p>
      </div>
      <Button size="xs" variant="outline" disabled={busy} onClick={open}>
        {t("open")}
      </Button>
    </li>
  )
}
