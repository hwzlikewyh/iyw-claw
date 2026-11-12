"use client"

import { Suspense, useCallback, useEffect, useState } from "react"
import { useIywAccount } from "@/contexts/iyw-account-context"
import { onTransportChange } from "@/lib/transport"
import {
  CapabilityMarketShell,
  type CapabilityMarketSection,
} from "@/components/skills/market/capability-market-shell"
import { ConnectorMarketView } from "@/components/skills/market/connector-market-view"
import { PluginMarketPreview } from "@/components/skills/market/plugin-market-preview"
import { SkillMarketView } from "@/components/skills/market/view"
import { useWorkbenchRoute } from "@/contexts/workbench-route-context"
import { cn } from "@/lib/utils"

export function SkillMarketPage() {
  const { profile, status } = useIywAccount()
  const [transportRevision, setTransportRevision] = useState(0)
  useEffect(
    () => onTransportChange(() => setTransportRevision((value) => value + 1)),
    []
  )
  return (
    <SkillMarketContent
      key={JSON.stringify([
        profile?.user_id,
        profile?.org_code,
        profile?.workspace_pending,
        status,
        transportRevision,
      ])}
    />
  )
}

function SkillMarketContent() {
  const {
    skillMarketTarget,
    consumeSkillMarketTarget,
    openSkillMarket,
    connectorMarketRequested,
    consumeConnectorMarket,
    pluginMarketRequested,
    consumePluginMarket,
  } = useWorkbenchRoute()
  const [section, setSection] = useState<CapabilityMarketSection>("skills")
  const [visited, setVisited] = useState<Set<CapabilityMarketSection>>(
    () => new Set(["skills"])
  )

  const openSection = useCallback((next: CapabilityMarketSection) => {
    setVisited((current) => {
      if (current.has(next)) return current
      return new Set(current).add(next)
    })
    setSection(next)
  }, [])

  useEffect(() => {
    let active = true
    if (connectorMarketRequested || pluginMarketRequested || skillMarketTarget)
      queueMicrotask(() => {
        if (active) {
          if (skillMarketTarget) openSection("skills")
          else if (pluginMarketRequested) {
            openSection("plugins")
            consumePluginMarket()
          } else {
            openSection("connectors")
            consumeConnectorMarket()
          }
        }
      })
    return () => {
      active = false
    }
  }, [
    connectorMarketRequested,
    consumeConnectorMarket,
    pluginMarketRequested,
    consumePluginMarket,
    skillMarketTarget,
    openSection,
  ])

  return (
    <Suspense fallback={null}>
      <CapabilityMarketShell
        activeSection={section}
        onSectionChange={openSection}
      >
        <div className={cn("h-full", section !== "skills" && "hidden")}>
          <SkillMarketView
            navigationTarget={skillMarketTarget}
            onNavigationTargetConsumed={consumeSkillMarketTarget}
            onOpenConnectors={() => openSection("connectors")}
          />
        </div>
        {visited.has("connectors") ? (
          <div className={cn("h-full", section !== "connectors" && "hidden")}>
            <ConnectorMarketView />
          </div>
        ) : null}
        {visited.has("plugins") ? (
          <div className={cn("h-full", section !== "plugins" && "hidden")}>
            <PluginMarketPreview
              onOpenPlugin={(slug) => {
                openSection("skills")
                openSkillMarket(slug)
              }}
            />
          </div>
        ) : null}
      </CapabilityMarketShell>
    </Suspense>
  )
}
