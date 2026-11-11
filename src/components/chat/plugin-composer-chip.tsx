"use client"

import { X } from "lucide-react"
import { useTranslations } from "next-intl"
import { PluginContextBadge } from "./plugin-context-badge"
import { requestPluginApplicationOpen } from "@/lib/plugin-preparation-events"
import { toast } from "sonner"
import {
  clearComposerPlugin,
  usePluginComposerContext,
} from "@/lib/plugin-composer-context"

export function PluginComposerChip({ tabId }: { tabId?: string | null }) {
  const t = useTranslations("PluginPanels")
  const app = usePluginComposerContext((state) =>
    tabId ? state.entries[tabId]?.selected : undefined
  )
  if (!app || !tabId) return null
  return (
    <div className="px-3 pt-2" data-plugin-composer-selection>
      <span className="inline-flex max-w-full items-center gap-1 rounded-md">
        <PluginContextBadge
          label={app.label}
          className="min-h-8 max-sm:min-h-11"
          onClick={() => {
            if (!requestPluginApplicationOpen(app))
              toast.error(t("openUnavailable"))
          }}
        />
        <button
          type="button"
          className="flex min-h-7 min-w-7 items-center justify-center rounded-md hover:bg-muted max-sm:min-h-11 max-sm:min-w-11"
          aria-label={t("removeSelection", { name: app.label })}
          onClick={() => clearComposerPlugin(tabId)}
        >
          <X className="size-3.5" />
        </button>
      </span>
    </div>
  )
}
