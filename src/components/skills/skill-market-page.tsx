"use client"

import { Suspense, useCallback, useEffect, useState } from "react"
import { useIywAccount } from "@/contexts/iyw-account-context"
import { onTransportChange } from "@/lib/transport"
import {
  CapabilityMarketShell,
  type CapabilityMarketSection,
} from "@/components/skills/market/capability-market-shell"
import { ConnectorMarketView } from "@/components/skills/market/connector-market-view"
import { SkillMarketView } from "@/components/skills/market/view"
import { useWorkbenchRoute } from "@/contexts/workbench-route-context"
import { cn } from "@/lib/utils"
import { CapabilityCatalogView } from "./market/capability-catalog-view"
import { Button } from "@/components/ui/button"
import {
  Dialog,
  DialogContent,
  DialogTitle,
  DialogDescription,
} from "@/components/ui/dialog"
import { useTranslations } from "next-intl"

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
    connectorMarketRequested,
    consumeConnectorMarket,
    pluginMarketRequested,
    consumePluginMarket,
  } = useWorkbenchRoute()
  const [section, setSection] = useState<CapabilityMarketSection>("all")
  const [chooseAction, setChooseAction] = useState<
    MarketViewAction["kind"] | null
  >(null)
  const sequence = useRef(0)
  const [actions, setActions] = useState<
    Partial<Record<CapabilityMarketSection, MarketViewAction>>
  >({})
  const [visited, setVisited] = useState<Set<CapabilityMarketSection>>(
    () => new Set(["all", "skills"])
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
          if (skillMarketTarget)
            openSection(
              skillMarketTarget.packageType === "plugin" ? "plugins" : "skills"
            )
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
        onAction={(kind) => {
          if (section === "all") {
            setChooseAction(kind)
            return
          }
          setActions((current) => ({
            ...current,
            [section]: { id: ++sequence.current, kind },
          }))
        }}
      >
        <div className={cn(section !== "all" && "hidden")}>
          <CapabilityCatalogView />
        </div>
        <Dialog
          open={chooseAction != null}
          onOpenChange={(open) => {
            if (!open) setChooseAction(null)
          }}
        >
          <DialogContent>
            <DialogTitle>{t("catalog.chooseType")}</DialogTitle>
            <DialogDescription>{t("catalog.chooseTypeHint")}</DialogDescription>
            <div className="flex flex-wrap gap-3">
              {(["skills", "connectors", "plugins"] as const).map((type) => (
                <Button
                  key={type}
                  variant="outline"
                  onClick={() => {
                    const kind = chooseAction
                    setChooseAction(null)
                    openSection(type)
                    if (kind)
                      setActions((current) => ({
                        ...current,
                        [type]: { id: ++sequence.current, kind },
                      }))
                  }}
                >
                  {t(`sections.${type}`)}
                </Button>
              ))}
            </div>
          </DialogContent>
        </Dialog>
        <div className={cn(section !== "skills" && "hidden")}>
          <SkillMarketView
            action={actions.skills}
            navigationTarget={
              skillMarketTarget?.packageType === "plugin"
                ? null
                : skillMarketTarget
            }
            onNavigationTargetConsumed={consumeSkillMarketTarget}
            onOpenConnectors={() => openSection("connectors")}
          />
        </div>
        {visited.has("connectors") ? (
          <div className={cn(section !== "connectors" && "hidden")}>
            <ConnectorMarketView action={actions.connectors} />
          </div>
        ) : null}
        {visited.has("plugins") ? (
          <div className={cn(section !== "plugins" && "hidden")}>
            <SkillMarketView
              action={actions.plugins}
              packageType="plugin"
              navigationTarget={
                skillMarketTarget?.packageType === "plugin"
                  ? skillMarketTarget
                  : null
              }
              onNavigationTargetConsumed={consumeSkillMarketTarget}
              onOpenConnectors={() => openSection("connectors")}
            />
          </div>
        ) : null}
      </CapabilityMarketShell>
    </Suspense>
  )
}
