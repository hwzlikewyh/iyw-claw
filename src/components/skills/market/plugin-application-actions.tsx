"use client"
import { useEffect, useState } from "react"
import { AppWindow } from "lucide-react"
import { useTranslations } from "next-intl"
import { Button } from "@/components/ui/button"
import { readPluginPanels, type PluginPanelChoice } from "@/lib/plugin-panel"
import {
  onPluginPreparation,
  requestPluginApplicationOpen,
} from "@/lib/plugin-preparation-events"

export function PluginApplicationActions({
  pluginId,
  conversationId,
}: {
  pluginId: string
  conversationId: number | null
}) {
  const t = useTranslations("PluginPanels")
  const { apps, error, setError } = useApplicationChoices(
    pluginId,
    conversationId
  )
  if (!apps.length && !error) return null
  return (
    <div className="space-y-2">
      {apps.map((app) => (
        <Button
          key={app.appKey}
          variant="outline"
          className="w-full justify-start rounded-md"
          onClick={() => {
            if (!requestPluginApplicationOpen(app))
              setError(t("openUnavailable"))
          }}
        >
          <AppWindow className="size-4" />
          <span className="truncate">
            {t("openNamed", { name: app.label })}
          </span>
        </Button>
      ))}
      {error && (
        <p role="alert" className="break-words text-sm text-destructive">
          {error}
        </p>
      )}
    </div>
  )
}

function useApplicationChoices(
  pluginId: string,
  conversationId: number | null
) {
  const [apps, setApps] = useState<PluginPanelChoice[]>([])
  const [error, setError] = useState("")
  const [revision, setRevision] = useState(0)
  useEffect(
    () =>
      onPluginPreparation((event) => {
        if (
          event.pluginId === pluginId &&
          event.conversationId === conversationId &&
          !event.canceled
        )
          setRevision((value) => value + 1)
      }),
    [pluginId, conversationId]
  )
  useEffect(() => {
    let active = true
    void readPluginPanels(conversationId)
      .then((values) => {
        if (active) {
          setApps(values.filter((app) => app.pluginId === pluginId))
          setError("")
        }
      })
      .catch((cause) => {
        if (active) setError(String(cause))
      })
    return () => {
      active = false
    }
  }, [pluginId, conversationId, revision])
  return { apps, error, setError }
}
