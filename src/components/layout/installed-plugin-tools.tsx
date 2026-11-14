import { Loader2 } from "lucide-react"
import { useTranslations } from "next-intl"
import { useTabStore } from "@/stores/tab-store"
import { useWorkbenchRoute } from "@/contexts/workbench-route-context"
import { useComposerPlugins } from "@/components/chat/use-composer-plugins"
import { useComposerPluginSelection } from "@/components/chat/use-composer-plugin-selection"
import { selectComposerPlugin } from "@/lib/plugin-composer-context"
import { Button } from "@/components/ui/button"
import type { ComposerPluginChoice } from "@/lib/composer-plugins"
import type { PluginPanelChoice } from "@/lib/plugin-panel"
import type { usePluginPanels } from "./use-plugin-panels"
import { PluginIcon } from "./plugin-icon"
import { usePluginPresentation } from "./plugin-presentation"

type Panels = ReturnType<typeof usePluginPanels>
type PluginRow = {
  key: string
  slug: string
  label: string
  version: string
  app?: PluginPanelChoice
  tool?: ComposerPluginChoice
}

export function InstalledPluginTools({ panels }: { panels: Panels }) {
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
  panels: Panels
  catalog: ReturnType<typeof useComposerPlugins>
  selection: ReturnType<typeof useComposerPluginSelection>
}) {
  const t = useTranslations("PluginPanels")
  const metadata = usePluginPresentation()
  const tools = panels.loading || panels.catalogError ? [] : catalog.choices
  const choices = pluginRows(panels.choices, tools)
    .map((row) => ({
      ...row,
      label:
        row.app && row.label !== row.slug
          ? row.label
          : metadata.get(row.slug)?.label || row.label,
    }))
    .sort((left, right) => left.label.localeCompare(right.label))
  return (
    <>
      <PluginListStatus
        loading={catalog.loading || panels.loading}
        error={selection.error || catalog.error}
        empty={!choices.length && !panels.catalogError}
      />
      <ul
        aria-label={t("apps")}
        className="grid min-w-0 grid-cols-1 gap-x-4 gap-y-1 @min-[640px]:grid-cols-2"
      >
        {choices.map((row) => (
          <ToolRow
            key={row.key}
            row={row}
            busy={selection.busy || panels.busy}
            onSelect={() => {
              if (row.app) void panels.open(row.app)
              else if (row.tool) void selection.select(row.tool)
            }}
          />
        ))}
      </ul>
    </>
  )
}

function ToolRow({
  row,
  busy,
  onSelect,
}: {
  row: PluginRow
  busy: boolean
  onSelect: () => void
}) {
  const t = useTranslations("PluginPanels")
  return (
    <li className="flex min-h-16 min-w-0 items-center gap-3 rounded-md px-2 hover:bg-sidebar-accent/50">
      <PluginIcon slug={row.slug} label={row.label} />
      <div className="min-w-0 flex-1">
        <p className="truncate text-sm font-medium" title={row.label}>
          {row.label}
        </p>
        <p className="truncate text-xs text-muted-foreground">
          {t("installedVersion", { version: row.version })}
        </p>
      </div>
      <Button
        size="xs"
        variant="outline"
        disabled={busy}
        onClick={onSelect}
        title={
          row.app ? t("openNamed", { name: row.label }) : t("conversationOnly")
        }
      >
        {t(row.app ? "open" : "addToConversation")}
      </Button>
    </li>
  )
}

function PluginListStatus({
  loading,
  error,
  empty,
}: {
  loading: boolean
  error: string
  empty: boolean
}) {
  const t = useTranslations("PluginPanels")
  if (error)
    return (
      <p role="alert" className="break-words py-2 text-sm text-destructive">
        {error}
      </p>
    )
  if (loading)
    return (
      <p
        role="status"
        className="flex items-center gap-2 py-2 text-sm text-muted-foreground"
      >
        <Loader2 className="size-3.5 animate-spin" />
        {t("loading")}
      </p>
    )
  if (empty)
    return <p className="py-2 text-sm text-muted-foreground">{t("empty")}</p>
  return null
}

function pluginRows(
  apps: PluginPanelChoice[],
  tools: ComposerPluginChoice[]
): PluginRow[] {
  const rows: PluginRow[] = apps.map((app) => ({
    key: `${app.pluginSlug}:${app.appKey}`,
    slug: app.pluginSlug,
    label: app.label,
    version: app.version,
    app,
  }))
  const ids = new Set(apps.map((app) => app.pluginId))
  const slugs = new Set(apps.map((app) => app.pluginSlug))
  for (const tool of tools) {
    if (ids.has(tool.id) || slugs.has(tool.slug)) continue
    rows.push({
      key: tool.slug,
      slug: tool.slug,
      label: tool.label,
      version: tool.version,
      tool,
    })
  }
  return rows
}
