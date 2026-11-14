"use client"

import { useMemo } from "react"
import { useSearchParams } from "next/navigation"
import { useTranslations } from "next-intl"
import { AppTitleBar } from "./app-title-bar"
import { PluginAppHost } from "@/components/message/plugin-app-host"
import { usePluginPanelAuthority } from "./use-plugin-panel-authority"
import {
  useDetachedPluginLoad,
  useDetachedPluginSync,
  usePluginWindowClose,
  usePluginWindowRelay,
} from "./use-detached-plugin"
import { reportPluginWindowReady } from "@/lib/plugin-panel-window"
import type { PluginPanelTab } from "./use-plugin-panel-tabs"
import { getTransport } from "@/lib/transport"

export function DetachedPluginPanel() {
  const params = useSearchParams()
  const input = useMemo(
    () => ({
      conversationId: Number(params.get("conversationId")),
      pluginId: "",
      pluginSlug: params.get("pluginSlug") ?? "",
      appKey: params.get("appKey") ?? "",
      version: params.get("version") ?? "",
      label: params.get("label") ?? params.get("pluginSlug") ?? "",
      state: "available" as const,
    }),
    [params]
  )
  const connected = usePluginWindowRelay(
    params.get("relayId") ?? "",
    input.conversationId
  )
  const { tab, error } = useDetachedPluginLoad(input, connected)
  const t = useTranslations("PluginPanels")
  return (
    <main className="fixed inset-0 flex min-h-0 flex-col bg-background text-foreground">
      <AppTitleBar
        left={
          <span data-tauri-drag-region className="truncate text-sm">
            {input.label}
          </span>
        }
      />
      {error ? (
        <p role="alert" className="p-4 text-sm text-destructive">
          {error}
        </p>
      ) : tab ? (
        <DetachedContent tab={tab} />
      ) : (
        <p role="status" className="p-4 text-sm">
          {t("loading")}
        </p>
      )}
    </main>
  )
}

function DetachedContent({ tab }: { tab: PluginPanelTab }) {
  const t = useTranslations("PluginPanels")
  const revoked = usePluginPanelAuthority(tab)
  const sync = useDetachedPluginSync(tab)
  const closeError = usePluginWindowClose(tab)
  return (
    <div className="flex min-h-0 flex-1 flex-col">
      {closeError && (
        <p role="alert" className="p-2 text-sm text-destructive">
          {closeError}
        </p>
      )}
      {revoked || sync.forbidden ? (
        <p role="alert" className="p-4">
          {t("revoked")}
        </p>
      ) : (
        <>
          {sync.error && (
            <p role="status" className="p-2 text-xs">
              {t("syncFailed")}
            </p>
          )}
          <div className="min-h-0 flex-1">
            <PluginAppHost
              {...tab.response}
              fill
              toolUpdate={sync.update}
              onReady={() => {
                void reportPluginWindowReady().catch(console.error)
              }}
              onFailure={() => {
                void getTransport()
                  .call("plugin_app_teardown", {
                    request: {
                      instanceId: tab.id,
                      conversationId: tab.conversationId,
                    },
                  })
                  .finally(() => reportPluginWindowReady(t("openUnavailable")))
                  .catch(console.error)
              }}
            />
          </div>
        </>
      )}
    </div>
  )
}
