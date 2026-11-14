import { Boxes } from "lucide-react"
import { useTranslations } from "next-intl"
import { useTabStore } from "@/stores/tab-store"
import { useWorkbenchRoute } from "@/contexts/workbench-route-context"
import { useComposerPlugins } from "@/components/chat/use-composer-plugins"
import { useComposerPluginSelection } from "@/components/chat/use-composer-plugin-selection"
import { selectComposerPlugin } from "@/lib/plugin-composer-context"
import { Button } from "@/components/ui/button"
import type { ComposerPluginChoice } from "@/lib/composer-plugins"
import type { usePluginPanels } from "./use-plugin-panels"

export function InstalledPluginTools({
  panels,
}: {
  panels: ReturnType<typeof usePluginPanels>
}) {
  const route = useWorkbenchRoute()
  const tab = useTabStore((state) =>
    state.tabs.find((item) => item.id === state.activeTabId)
  )
  const catalog = useComposerPlugins({
    open: true,
    agentType: tab?.agentType ?? null,
    workspacePath: tab?.workingDir ?? null,
  })
  const selection = useComposerPluginSelection({
    scopeKey: catalog.scopeKey,
    tabId: tab?.id ?? null,
    onSelect: (plugin) => {
      if (!tab) return
      selectComposerPlugin(tab.id, {
        pluginId: plugin.id,
        pluginSlug: plugin.slug,
        version: plugin.version,
        label: plugin.label,
        appKey: "",
        state: "available",
      })
      route.openConversations()
    },
  })
  return (
    <PluginToolsList panels={panels} catalog={catalog} selection={selection} />
  )
}

function PluginToolsList({
  panels,
  catalog,
  selection,
}: {
  panels: ReturnType<typeof usePluginPanels>
  catalog: ReturnType<typeof useComposerPlugins>
  selection: ReturnType<typeof useComposerPluginSelection>
}) {
  const t = useTranslations("PluginPanels")
  const choices = catalog.choices.filter(
    (plugin) => !panels.choices.some((app) => app.pluginId === plugin.id)
  )
  return (
    <>
      {catalog.loading && !panels.loading && (
        <p role="status" className="py-2 text-sm text-muted-foreground">
          {t("loading")}
        </p>
      )}
      {(catalog.error || selection.error) && (
        <p role="alert" className="py-2 text-sm text-destructive">
          {selection.error || catalog.error}
        </p>
      )}
      {!catalog.loading &&
        !panels.loading &&
        !catalog.error &&
        !panels.choices.length &&
        !choices.length && (
          <p className="py-2 text-sm text-muted-foreground">{t("empty")}</p>
        )}
      <ul className="grid grid-cols-1 gap-x-4 gap-y-1 @min-[480px]:grid-cols-2">
        {choices.map((plugin) => (
          <ToolRow
            key={plugin.id}
            plugin={plugin}
            busy={selection.busy || panels.busy}
            onSelect={() => void selection.select(plugin)}
          />
        ))}
      </ul>
    </>
  )
}

function ToolRow({
  plugin,
  busy,
  onSelect,
}: {
  plugin: ComposerPluginChoice
  busy: boolean
  onSelect: () => void
}) {
  const t = useTranslations("PluginPanels")
  return (
    <li className="flex min-h-16 min-w-0 items-center gap-3">
      <Boxes className="size-5 shrink-0 text-muted-foreground" />
      <div className="min-w-0 flex-1">
        <p className="truncate text-sm font-medium">{plugin.label}</p>
        <p className="text-xs text-muted-foreground">
          {t("installedVersion", { version: plugin.version })}
        </p>
      </div>
      <Button size="xs" variant="outline" disabled={busy} onClick={onSelect}>
        {t("use")}
      </Button>
    </li>
  )
}
