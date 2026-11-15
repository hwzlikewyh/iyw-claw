"use client"

import { createPortal } from "react-dom"
import { useTranslations } from "next-intl"
import { useAuxPanelContext } from "@/contexts/aux-panel-context"
import { useWorkbenchRoute } from "@/contexts/workbench-route-context"
import { useBrowser } from "@/contexts/browser-context"
import { useInteractivePage } from "@/contexts/interactive-page-context"
import { Button } from "@/components/ui/button"
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from "@/components/ui/tooltip"
import { PluginIcon } from "./plugin-icon"
import type { PluginPanelTab, usePluginPanels } from "./use-plugin-panels"

export function CollapsedPluginRail({
  slot,
  panels,
}: {
  slot: HTMLElement | null
  panels: ReturnType<typeof usePluginPanels>
}) {
  const { isOpen, restored, openTab } = useAuxPanelContext()
  const route = useWorkbenchRoute()
  const { closeBrowser } = useBrowser()
  const { closePanel } = useInteractivePage()
  const t = useTranslations("PluginPanels")
  const tabs = panels.tabs.filter(
    (tab) => tab.conversationId === panels.conversationId
  )
  if (!slot || isOpen || !restored || !tabs.length) return null
  return createPortal(
    <nav
      aria-label={t("openedApps")}
      className="flex h-full w-11 flex-col items-center gap-1 overflow-y-auto border-l bg-sidebar py-2"
    >
      {tabs.map((tab) => (
        <PluginRailButton
          key={tab.id}
          tab={tab}
          moving={panels.detachingId === tab.id}
          open={() => {
            closePanel()
            void closeBrowser()
            route.openConversations()
            openTab(`app:${tab.id}`)
          }}
        />
      ))}
    </nav>,
    slot
  )
}

function PluginRailButton({
  tab,
  moving,
  open,
}: {
  tab: PluginPanelTab
  moving: boolean
  open: () => void
}) {
  const t = useTranslations("PluginPanels")
  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <Button
          variant="ghost"
          size="icon"
          className="size-9 shrink-0 rounded-md"
          aria-label={t("openNamed", { name: tab.label })}
          disabled={moving}
          onClick={open}
        >
          <PluginIcon slug={tab.pluginSlug} label={tab.label} />
        </Button>
      </TooltipTrigger>
      <TooltipContent side="left" sideOffset={6}>
        {tab.label}
      </TooltipContent>
    </Tooltip>
  )
}
