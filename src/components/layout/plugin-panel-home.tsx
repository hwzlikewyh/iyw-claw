"use client"

import { AppWindow, ChevronRight, RefreshCw } from "lucide-react"
import { useTranslations } from "next-intl"
import { useTabStore } from "@/contexts/tab-context"
import { usePluginWorkspace } from "@/components/skills/market/use-plugin-workspace"
import { Button } from "@/components/ui/button"
import type { PluginPanelChoice } from "@/lib/plugin-panel"
import type { usePluginPanels } from "./use-plugin-panels"
import { usePluginMarketNavigation } from "./use-plugin-market-navigation"
import { CloudPlugins } from "./plugin-panel-cloud"

type Panels = ReturnType<typeof usePluginPanels>

export function PluginPanelHome({ panels }: { panels: Panels }) {
  const t = useTranslations("PluginPanels")
  const route = usePluginMarketNavigation()
  const tabId = useTabStore((store) => store.activeTabId)
  const workspace = usePluginWorkspace(tabId)
  const open = (app: PluginPanelChoice) => {
    if (app.state === "context_required") void workspace.bind()
    else if (app.state !== "available") route.openSkillMarket(app.pluginSlug)
    else void panels.open(app)
  }
  return (
    <section className="px-3 pb-4 pt-2">
      <PluginListHeading refresh={panels.refresh} />
      {panels.loading && (
        <p role="status" className="py-2 text-sm text-muted-foreground">
          {t("loading")}
        </p>
      )}
      {(panels.error || workspace.error) && (
        <p role="alert" className="break-words py-2 text-sm text-destructive">
          {panels.error === "PluginPanels.limit"
            ? t("limit")
            : panels.error || workspace.error}
        </p>
      )}
      {!panels.loading && !panels.choices.length && (
        <p className="py-2 text-sm text-muted-foreground">{t("empty")}</p>
      )}
      <ul className="grid grid-cols-1 gap-x-4 gap-y-1 @min-[480px]:grid-cols-2">
        {panels.choices.map((app) => (
          <InstalledApp
            key={`${app.pluginSlug}:${app.appKey}`}
            app={app}
            busy={panels.busy || workspace.busy}
            open={() => open(app)}
          />
        ))}
      </ul>
      <CloudPlugins installed={panels.choices.map((app) => app.pluginSlug)} />
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
  const action =
    app.state === "available"
      ? "open"
      : app.state === "context_required"
        ? "prepareContext"
        : "prepare"
  return (
    <li className="flex min-h-16 min-w-0 items-center gap-3">
      <AppWindow className="size-5 shrink-0 text-muted-foreground" />
      <div className="min-w-0 flex-1">
        <p className="truncate text-sm font-medium" title={app.label}>
          {app.label}
        </p>
        <p className="truncate text-xs text-muted-foreground">
          {t(`state.${app.state}`)}
        </p>
      </div>
      <Button size="xs" variant="outline" disabled={busy} onClick={open}>
        {t(action)}
      </Button>
    </li>
  )
}
