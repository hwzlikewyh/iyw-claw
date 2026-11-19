"use client"

import { useEditorState } from "@tiptap/react"
import type { Editor } from "@tiptap/core"
import { useTranslations } from "next-intl"
import { toast } from "sonner"
import { usePluginPresentation } from "@/components/layout/plugin-presentation"
import { requestPluginApplicationOpen } from "@/lib/plugin-preparation-events"
import {
  clearComposerPlugin,
  usePluginComposerContext,
} from "@/lib/plugin-composer-context"
import {
  composerSources,
  removeComposerSource,
} from "./composer/source-references"
import { ComposerSourceChip } from "./composer-source-chip"

export function PluginComposerChip({
  tabId,
  editor = null,
}: {
  tabId?: string | null
  editor?: Editor | null
}) {
  const t = useTranslations("PluginPanels")
  const items = usePluginPresentation()
  const app = usePluginComposerContext((state) =>
    tabId ? state.entries[tabId]?.selected : undefined
  )
  const references =
    useEditorState({
      editor,
      selector: ({ editor }) => composerSources(editor),
    }) ?? []
  const sources = [...references]
  if (
    app &&
    !sources.some((s) => s.kind === "plugin" && s.slug === app.pluginSlug)
  ) {
    sources.push({ kind: "plugin", slug: app.pluginSlug, label: app.label })
  }
  return (
    <>
      {sources.map((source) => {
        const panel =
          source.kind === "plugin" && app?.pluginSlug === source.slug
            ? app
            : undefined
        const item =
          items.get(
            source.marketId ? "market:" + source.marketId : source.slug
          ) ?? items.get(source.slug)
        return (
          <ComposerSourceChip
            key={source.kind + ":" + source.slug}
            kind={source.kind}
            label={source.label}
            src={item?.iconUrl}
            onOpen={
              panel?.appKey && panel.state === "available"
                ? () => {
                    if (!requestPluginApplicationOpen(panel))
                      toast.error(t("openUnavailable"))
                  }
                : undefined
            }
            removeLabel={t("removeSelection", { name: source.label })}
            onRemove={() => {
              removeComposerSource(editor, source)
              if (panel && tabId) clearComposerPlugin(tabId)
            }}
          />
        )
      })}
    </>
  )
}
