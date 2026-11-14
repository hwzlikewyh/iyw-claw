import { useEffect, useState } from "react"
import { useAuxPanelContext } from "@/contexts/aux-panel-context"
import { readPluginPanels, type PluginPanelChoice } from "@/lib/plugin-panel"
import { extractAppCommandError } from "@/lib/app-error"
import type { getTransport } from "@/lib/transport"
import { onSkillMarketInstallationsChanged } from "@/lib/skill-market-installation-events"

export function usePluginPanelChoices(
  conversationId: number | null,
  transport: ReturnType<typeof getTransport>
) {
  const [choices, setChoices] = useState<PluginPanelChoice[]>([])
  const [loading, setLoading] = useState(true),
    [error, setError] = useState("")
  const [revision, setRevision] = useState(0)
  const { isOpen } = useAuxPanelContext()
  useEffect(() => {
    const refresh = () => setRevision((value) => value + 1)
    window.addEventListener("focus", refresh)
    const unsubscribe = onSkillMarketInstallationsChanged(refresh)
    const disconnect = transport.onReconnect?.(refresh)
    return () => {
      window.removeEventListener("focus", refresh)
      unsubscribe()
      disconnect?.()
    }
  }, [transport])
  useEffect(() => {
    if (!isOpen) return
    let alive = true
    const timer = setTimeout(() => {
      setLoading(true)
      setError("")
      void readPluginPanels(conversationId, transport)
        .then((value) => {
          if (alive) setChoices(value)
        })
        .catch((cause) => {
          if (alive) {
            setChoices([])
            setError(pluginPanelError(cause))
          }
        })
        .finally(() => {
          if (alive) setLoading(false)
        })
    }, 0)
    return () => {
      alive = false
      clearTimeout(timer)
    }
  }, [conversationId, isOpen, revision, transport])
  return {
    choices,
    loading,
    catalogError: error,
    refresh: () => setRevision((value) => value + 1),
  }
}

export const pluginPanelError = (cause: unknown) =>
  extractAppCommandError(cause)?.message || String(cause)
