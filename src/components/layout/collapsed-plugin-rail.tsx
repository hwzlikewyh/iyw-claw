"use client"

import { createPortal } from "react-dom"
import { useTranslations } from "next-intl"
import { useAuxPanelContext } from "@/contexts/aux-panel-context"
import { useWorkbenchRoute } from "@/contexts/workbench-route-context"
import { useBrowser } from "@/contexts/browser-context"
import { useInteractivePage } from "@/contexts/interactive-page-context"
import { CollapsedOverlayChip } from "@/components/chat/collapsed-overlay-chip"
import { isPluginPanelVisibleForConversation } from "@/lib/plugin-panel"
import { PluginIcon } from "./plugin-icon"
import { usePluginPresentation } from "./plugin-presentation"
import type { PluginPanelTab, usePluginPanels } from "./use-plugin-panels"

export function CollapsedPluginRail({
  slot,
  panels,
}: {
  slot: HTMLElement | null
  panels: ReturnType<typeof usePluginPanels>
}) {
  const { isOpen, restored, setActiveTab, setOpen } = useAuxPanelContext()
  const route = useWorkbenchRoute()
  const { closeBrowser } = useBrowser()
  const { closePanel } = useInteractivePage()
  const t = useTranslations("PluginPanels")
  const tabs = panels.tabs.filter((tab) =>
    isPluginPanelVisibleForConversation(tab, panels.conversationId)
  )
  if (!slot || isOpen || !restored || !tabs.length) return null
  return createPortal(
    <nav
      aria-label={t("openedApps")}
      className="pointer-events-none absolute end-2 top-24 bottom-3 flex max-w-[calc(100%-2rem)] flex-col items-end gap-2 overflow-y-auto overscroll-contain ps-2 pb-2 [scrollbar-width:none] lg:end-8 [&::-webkit-scrollbar]:hidden"
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
            setActiveTab(`app:${tab.id}`)
            setOpen(true)
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
  const name = usePluginPresentation().get(tab.pluginSlug)?.label || tab.label
  return (
    <CollapsedOverlayChip
      edge="end"
      icon={
        <PluginIcon slug={tab.pluginSlug} label={name} className="size-6" />
      }
      summary={t("openNamed", { name })}
      disabled={moving}
      onClick={open}
    />
  )
}
