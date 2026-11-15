"use client"

import { createContext, useContext, useEffect, useState } from "react"
import type { ReactNode } from "react"
import { loadComposerPluginCatalog } from "@/lib/composer-plugins"
import { onSkillMarketInstallationsChanged } from "@/lib/skill-market-installation-events"
import { getTransport } from "@/lib/transport"
import { toErrorMessage } from "@/lib/app-error"

type Presentation = { label: string; iconUrl: string | null }
const PresentationContext = createContext(new Map<string, Presentation>())

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
  const [items, setItems] = useState(new Map<string, Presentation>())
  useEffect(() => {
    let active = true
    let revision = 0
    const transport = getTransport()
    const refresh = () => {
      const request = ++revision
      void loadComposerPluginCatalog()
        .then((catalog) => {
          if (!active || request !== revision || transport !== getTransport())
            return
          setItems(presentations(catalog))
        })
        .catch((cause) => {
          if (active && request === revision)
            console.warn("[plugin-presentation] metadata unavailable", {
              error: toErrorMessage(cause),
            })
        })
    }
    refresh()
    window.addEventListener("focus", refresh)
    const unsubscribe = onSkillMarketInstallationsChanged(refresh)
    const reconnect = transport.onReconnect?.(refresh)
    return () => {
      active = false
      window.removeEventListener("focus", refresh)
      unsubscribe()
      reconnect?.()
    }
  }, [])
  return items
}

export const usePluginPresentation = () => useContext(PresentationContext)

function presentations(
  catalog: Awaited<ReturnType<typeof loadComposerPluginCatalog>>
) {
  return new Map(
    catalog.map((item) => [
      item.slug,
      {
        label: item.displayName,
        iconUrl: item.iconUrl,
      },
    ])
  )
}
