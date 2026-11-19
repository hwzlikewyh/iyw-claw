"use client"

import { createContext, useContext, useEffect, useState } from "react"
import { useIywAccount } from "@/contexts/iyw-account-context"
import type { ReactNode } from "react"
import { loadInstalledComposerCatalog } from "@/lib/composer-plugins"
import { onSkillMarketInstallationsChanged } from "@/lib/skill-market-installation-events"
import { getTransport, onTransportChange } from "@/lib/transport"
import { toErrorMessage } from "@/lib/app-error"

type Presentation = {
  label: string
  iconUrl: string | null
  kind: "skill" | "expert" | "plugin"
}
const EMPTY_PRESENTATIONS = new Map<string, Presentation>()
const PresentationContext = createContext(EMPTY_PRESENTATIONS)

// 只补充展示信息；已安装版本与可打开的页面仍以本地目录为准。
export function PluginPresentationProvider({
  children,
}: {
  children: ReactNode
}) {
  const items = usePresentationCatalog()
  return (
    <PresentationContext.Provider value={items}>
      {children}
    </PresentationContext.Provider>
  )
}

function usePresentationCatalog() {
  const { profile, status } = useIywAccount()
  const [transport, setTransport] = useState(getTransport)
  useEffect(() => onTransportChange(() => setTransport(getTransport())), [])
  const scope = JSON.stringify([
    profile?.user_id,
    profile?.org_code,
    profile?.workspace_pending,
    status,
  ])
  const [result, setResult] = useState<{
    scope: string
    transport: typeof transport
    items: Map<string, Presentation>
  } | null>(null)
  useEffect(() => {
    let active = true
    let revision = 0
    const refresh = () => {
      const request = ++revision
      void loadInstalledComposerCatalog()
        .then((catalog) => {
          if (!active || request !== revision || transport !== getTransport())
            return
          setResult({ scope, transport, items: presentations(catalog) })
        })
        .catch((cause) => {
          if (active && request === revision)
            console.warn("[plugin-presentation] metadata unavailable", {
              error: toErrorMessage(cause),
            })
        })
    }
    refresh()
    const unsubscribe = onSkillMarketInstallationsChanged(refresh)
    const reconnect = transport.onReconnect?.(refresh)
    return () => {
      active = false
      unsubscribe()
      reconnect?.()
    }
  }, [scope, transport])
  return result?.scope === scope && result.transport === transport
    ? result.items
    : EMPTY_PRESENTATIONS
}

export const usePluginPresentation = () => useContext(PresentationContext)

function presentations(
  catalog: Awaited<ReturnType<typeof loadInstalledComposerCatalog>>
) {
  const items = new Map<string, Presentation>()
  for (const item of catalog) {
    const display = {
      kind: item.packageType,
      label: item.displayName,
      iconUrl: item.iconUrl,
    }
    items.set(item.slug, display)
    items.set("market:" + item.id, display)
  }
  return items
}
