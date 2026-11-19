"use client"

import { useEffect, useState } from "react"
import { mcpScanLocal } from "@/lib/api"
import { getTransport, onTransportChange } from "@/lib/transport"
import { onMarketCatalogChanged } from "@/lib/market-catalog-events"
import { usePluginPresentation } from "@/components/layout/plugin-presentation"

const empty = new Map<string, string>()
export function useConnectorIcons(enabled: boolean) {
  const catalog = usePluginPresentation()
  const [transport, setTransport] = useState(getTransport)
  const [icons, setIcons] = useState(empty)
  useEffect(() => onTransportChange(() => setTransport(getTransport())), [])
  useEffect(() => {
    if (!enabled) return
    let current = 0
    const refresh = () => {
      const request = ++current
      void mcpScanLocal()
        .then((servers) => {
          if (current !== request || transport !== getTransport()) return
          const next = new Map<string, string>()
          for (const server of servers) {
            const market = server.spec.iyw_market as
              | { icon_url?: unknown }
              | undefined
            const owner = server.sources?.find(
              (source) => source.kind === "plugin"
            )
            const url =
              market?.icon_url || catalog.get(owner?.ownerName ?? "")?.iconUrl
            if (typeof url === "string" && url.trim()) next.set(server.id, url)
          }
          setIcons(next)
        })
        .catch(() => {
          if (request === current) setIcons(empty)
        })
    }
    refresh()
    const unsubscribe = onMarketCatalogChanged(refresh)
    return () => {
      current++
      unsubscribe()
    }
  }, [enabled, catalog, transport])
  return enabled ? icons : empty
}
